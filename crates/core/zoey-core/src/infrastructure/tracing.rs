//! Request tracing and correlation IDs
//!
//! Provides distributed tracing support with:
//! - Trace and span ID generation
//! - Context propagation
//! - Request correlation
//! - Timing and metrics

use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{info_span, Span};
use uuid::Uuid;

/// Trace ID for distributed tracing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub Uuid);

impl TraceId {
    /// Generate a new trace ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse from string
    pub fn parse(s: &str) -> Result<Self> {
        let uuid = Uuid::parse_str(s)
            .map_err(|e| ZoeyError::validation(format!("Invalid trace ID: {}", e)))?;
        Ok(Self(uuid))
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Span ID for tracing within a trace
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub u64);

impl SpanId {
    /// Generate a new span ID
    pub fn new() -> Self {
        Self(rand::random())
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Request context for tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// Trace ID
    pub trace_id: TraceId,
    /// Current span ID
    pub span_id: SpanId,
    /// Parent span ID (if any)
    pub parent_span_id: Option<SpanId>,
    /// Request start time (as unix timestamp ms)
    #[serde(skip)]
    pub start_time: Option<Instant>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Baggage items (propagated across services)
    pub baggage: HashMap<String, String>,
}

impl RequestContext {
    /// Create a new root context
    pub fn new() -> Self {
        Self {
            trace_id: TraceId::new(),
            span_id: SpanId::new(),
            parent_span_id: None,
            start_time: Some(Instant::now()),
            metadata: HashMap::new(),
            baggage: HashMap::new(),
        }
    }

    /// Create a child context
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: SpanId::new(),
            parent_span_id: Some(self.span_id),
            start_time: Some(Instant::now()),
            metadata: HashMap::new(),
            baggage: self.baggage.clone(),
        }
    }

    /// Set metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set baggage item (propagated to child contexts)
    pub fn with_baggage(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.baggage.insert(key.into(), value.into());
        self
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time.map(|t| t.elapsed()).unwrap_or_default()
    }

    /// Create a tracing span for this context
    pub fn span(&self, name: &str) -> Span {
        info_span!(
            "request",
            trace_id = %self.trace_id,
            span_id = %self.span_id,
            parent_span_id = ?self.parent_span_id.map(|s| s.to_string()),
            name = name
        )
    }

    /// Convert to HTTP headers for propagation
    pub fn to_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert("X-Trace-Id".to_string(), self.trace_id.to_string());
        headers.insert("X-Span-Id".to_string(), self.span_id.to_string());
        if let Some(parent) = self.parent_span_id {
            headers.insert("X-Parent-Span-Id".to_string(), parent.to_string());
        }

        // Add baggage
        for (key, value) in &self.baggage {
            headers.insert(format!("X-Baggage-{}", key), value.clone());
        }

        headers
    }

    /// Parse from HTTP headers
    pub fn from_headers(headers: &HashMap<String, String>) -> Result<Self> {
        let trace_id = headers
            .get("X-Trace-Id")
            .map(|s| TraceId::parse(s))
            .transpose()?
            .unwrap_or_else(TraceId::new);

        let parent_span_id = headers
            .get("X-Span-Id")
            .and_then(|s| s.parse::<u64>().ok())
            .map(SpanId);

        let mut baggage = HashMap::new();
        for (key, value) in headers {
            if let Some(baggage_key) = key.strip_prefix("X-Baggage-") {
                baggage.insert(baggage_key.to_string(), value.clone());
            }
        }

        Ok(Self {
            trace_id,
            span_id: SpanId::new(),
            parent_span_id,
            start_time: Some(Instant::now()),
            metadata: HashMap::new(),
            baggage,
        })
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Span event for recording events within a span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name
    pub name: String,
    /// Event timestamp (unix ms)
    pub timestamp: i64,
    /// Event attributes
    pub attributes: HashMap<String, serde_json::Value>,
}

/// Completed span information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedSpan {
    /// Trace ID
    pub trace_id: TraceId,
    /// Span ID
    pub span_id: SpanId,
    /// Parent span ID
    pub parent_span_id: Option<SpanId>,
    /// Operation name
    pub operation_name: String,
    /// Start time (unix ms)
    pub start_time: i64,
    /// End time (unix ms)
    pub end_time: i64,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Status
    pub status: SpanStatus,
    /// Attributes
    pub attributes: HashMap<String, serde_json::Value>,
    /// Events
    pub events: Vec<SpanEvent>,
}

/// Span status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    /// Operation completed successfully
    Ok,
    /// Operation failed
    Error,
    /// Operation was cancelled
    Cancelled,
}

/// Trace collector for aggregating spans
pub struct TraceCollector {
    spans: Arc<RwLock<HashMap<TraceId, Vec<CompletedSpan>>>>,
    max_traces: usize,
}

impl TraceCollector {
    /// Create a new trace collector
    pub fn new(max_traces: usize) -> Self {
        Self {
            spans: Arc::new(RwLock::new(HashMap::new())),
            max_traces,
        }
    }

    /// Record a completed span
    pub fn record_span(&self, span: CompletedSpan) {
        let mut spans = self.spans.write().unwrap();

        // Enforce max traces limit
        if spans.len() >= self.max_traces && !spans.contains_key(&span.trace_id) {
            // Remove oldest trace
            if let Some(oldest_key) = spans.keys().next().cloned() {
                spans.remove(&oldest_key);
            }
        }

        spans
            .entry(span.trace_id)
            .or_insert_with(Vec::new)
            .push(span);
    }

    /// Get all spans for a trace
    pub fn get_trace(&self, trace_id: TraceId) -> Option<Vec<CompletedSpan>> {
        self.spans.read().unwrap().get(&trace_id).cloned()
    }

    /// Get recent traces
    pub fn get_recent_traces(&self, limit: usize) -> Vec<(TraceId, Vec<CompletedSpan>)> {
        self.spans
            .read()
            .unwrap()
            .iter()
            .take(limit)
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    /// Clear all traces
    pub fn clear(&self) {
        self.spans.write().unwrap().clear();
    }
}

impl Default for TraceCollector {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Span builder for creating spans with fluent API
pub struct SpanBuilder {
    context: RequestContext,
    operation_name: String,
    attributes: HashMap<String, serde_json::Value>,
    events: Vec<SpanEvent>,
}

impl SpanBuilder {
    /// Create a new span builder
    pub fn new(context: RequestContext, operation_name: impl Into<String>) -> Self {
        Self {
            context,
            operation_name: operation_name.into(),
            attributes: HashMap::new(),
            events: Vec::new(),
        }
    }

    /// Add an attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.attributes.insert(key.into(), v);
        }
        self
    }

    /// Add an event
    pub fn with_event(mut self, name: impl Into<String>) -> Self {
        self.events.push(SpanEvent {
            name: name.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            attributes: HashMap::new(),
        });
        self
    }

    /// Finish the span with success
    pub fn finish_ok(self) -> CompletedSpan {
        self.finish(SpanStatus::Ok)
    }

    /// Finish the span with error
    pub fn finish_error(self) -> CompletedSpan {
        self.finish(SpanStatus::Error)
    }

    /// Finish the span with status
    fn finish(self, status: SpanStatus) -> CompletedSpan {
        let end_time = chrono::Utc::now().timestamp_millis();
        let start_time = end_time - self.context.elapsed().as_millis() as i64;

        CompletedSpan {
            trace_id: self.context.trace_id,
            span_id: self.context.span_id,
            parent_span_id: self.context.parent_span_id,
            operation_name: self.operation_name,
            start_time,
            end_time,
            duration_ms: self.context.elapsed().as_millis() as u64,
            status,
            attributes: self.attributes,
            events: self.events,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id_generation() {
        let id1 = TraceId::new();
        let id2 = TraceId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_request_context_child() {
        let parent = RequestContext::new();
        let child = parent.child();

        assert_eq!(parent.trace_id, child.trace_id);
        assert_eq!(Some(parent.span_id), child.parent_span_id);
        assert_ne!(parent.span_id, child.span_id);
    }

    #[test]
    fn test_baggage_propagation() {
        let parent = RequestContext::new()
            .with_baggage("user_id", "123")
            .with_baggage("tenant", "acme");

        let child = parent.child();

        assert_eq!(child.baggage.get("user_id"), Some(&"123".to_string()));
        assert_eq!(child.baggage.get("tenant"), Some(&"acme".to_string()));
    }

    #[test]
    fn test_header_propagation() {
        let ctx = RequestContext::new().with_baggage("user_id", "123");

        let headers = ctx.to_headers();
        let restored = RequestContext::from_headers(&headers).unwrap();

        assert_eq!(ctx.trace_id, restored.trace_id);
        assert_eq!(restored.baggage.get("user_id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_span_builder() {
        let ctx = RequestContext::new();
        let span = SpanBuilder::new(ctx, "test_operation")
            .with_attribute("key", "value")
            .with_event("started")
            .finish_ok();

        assert_eq!(span.operation_name, "test_operation");
        assert_eq!(span.status, SpanStatus::Ok);
    }

    #[test]
    fn test_trace_collector() {
        let collector = TraceCollector::new(10);
        let ctx = RequestContext::new();

        let span = SpanBuilder::new(ctx.clone(), "op1").finish_ok();
        collector.record_span(span);

        let trace = collector.get_trace(ctx.trace_id).unwrap();
        assert_eq!(trace.len(), 1);
    }
}
