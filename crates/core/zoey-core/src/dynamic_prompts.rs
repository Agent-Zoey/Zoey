//! Dynamic prompt execution system with schema validation
//!
//! This module provides a framework for schema-driven prompt execution with:
//! - Schema validation with retries
//! - Token estimation
//! - Metrics tracking
//! - LRU cache eviction
//! - Handlebars state injection
//! - XML/JSON parsing with validation codes
//!
//! Based on PR #6113: https://github.com/zoeyOS/zoey/pull/6113

use crate::templates::TemplateEngine;
use crate::types::State;
use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, instrument, warn};

/// Schema row defining expected output field
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaRow {
    /// Field name
    pub field: String,

    /// Field description
    pub description: String,

    /// Field type
    #[serde(rename = "type")]
    pub field_type: SchemaType,

    /// Whether field is required
    #[serde(default = "default_true")]
    pub required: bool,

    /// Example value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,

    /// Validation regex
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Schema field type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaType {
    /// String value
    String,
    /// Number value
    Number,
    /// Boolean value
    Boolean,
    /// Array value
    Array,
    /// Object value
    Object,
    /// UUID value
    Uuid,
    /// Enum value
    Enum,
}

/// Validation level for schema checking
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationLevel {
    /// No validation
    None,
    /// Basic validation (types only)
    Basic,
    /// Strict validation (types + required fields)
    Strict,
    /// Maximum validation (types + required + regex)
    Maximum,
}

impl Default for ValidationLevel {
    fn default() -> Self {
        ValidationLevel::Strict
    }
}

/// Dynamic prompt execution options
#[derive(Debug, Clone)]
pub struct DynamicPromptOptions {
    /// Model to use (defaults from model size)
    pub model: Option<String>,

    /// Model size hint
    pub model_size: Option<String>,

    /// Temperature
    pub temperature: Option<f32>,

    /// Max tokens
    pub max_tokens: Option<usize>,

    /// Validation level
    pub validation_level: ValidationLevel,

    /// Maximum retries on validation failure
    pub max_retries: usize,

    /// Cache key override
    pub key: Option<String>,

    /// Force specific format (xml or json)
    pub force_format: Option<ResponseFormat>,

    /// Stop sequences
    pub stop: Option<Vec<String>>,
}

impl Default for DynamicPromptOptions {
    fn default() -> Self {
        Self {
            model: None,
            model_size: None,
            temperature: Some(0.7),
            max_tokens: Some(150),
            validation_level: ValidationLevel::Strict,
            max_retries: 3,
            key: None,
            force_format: None,
            stop: None,
        }
    }
}

/// Response format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    /// XML format
    Xml,
    /// JSON format
    Json,
}

/// Execution metrics for schema-based prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaMetrics {
    /// Schema hash/key
    pub schema_key: String,

    /// Total executions
    pub execution_count: usize,

    /// Successful validations
    pub success_count: usize,

    /// Failed validations
    pub failure_count: usize,

    /// Total retries needed
    pub retry_count: usize,

    /// Average response time (ms)
    pub avg_response_time_ms: f64,

    /// Last execution timestamp
    pub last_execution: i64,

    /// Validation code usage
    pub validation_codes_used: HashMap<String, usize>,
}

impl SchemaMetrics {
    fn new(schema_key: String) -> Self {
        Self {
            schema_key,
            execution_count: 0,
            success_count: 0,
            failure_count: 0,
            retry_count: 0,
            avg_response_time_ms: 0.0,
            last_execution: chrono::Utc::now().timestamp_millis(),
            validation_codes_used: HashMap::new(),
        }
    }

    fn record_execution(&mut self, success: bool, retries: usize, duration_ms: u128) {
        self.execution_count += 1;
        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
        self.retry_count += retries;

        // Update average response time
        let total_time = self.avg_response_time_ms * (self.execution_count - 1) as f64;
        self.avg_response_time_ms = (total_time + duration_ms as f64) / self.execution_count as f64;

        self.last_execution = chrono::Utc::now().timestamp_millis();
    }
}

/// Model execution metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMetrics {
    /// Model identifier
    pub model: String,

    /// Total calls
    pub call_count: usize,

    /// Total tokens used (estimated)
    pub total_tokens: usize,

    /// Average latency (ms)
    pub avg_latency_ms: f64,

    /// Error count
    pub error_count: usize,
}

impl ModelMetrics {
    fn new(model: String) -> Self {
        Self {
            model,
            call_count: 0,
            total_tokens: 0,
            avg_latency_ms: 0.0,
            error_count: 0,
        }
    }

    fn record_call(&mut self, tokens: usize, latency_ms: u128, success: bool) {
        self.call_count += 1;
        self.total_tokens += tokens;

        if !success {
            self.error_count += 1;
        }

        let total_time = self.avg_latency_ms * (self.call_count - 1) as f64;
        self.avg_latency_ms = (total_time + latency_ms as f64) / self.call_count as f64;
    }
}

/// Dynamic prompt executor with metrics and caching
pub struct DynamicPromptExecutor {
    /// Schema metrics by cache key
    schema_metrics: Arc<RwLock<HashMap<String, SchemaMetrics>>>,

    /// Model metrics
    model_metrics: Arc<RwLock<HashMap<String, ModelMetrics>>>,

    /// Maximum cache entries (LRU eviction)
    max_entries: usize,

    /// Template engine
    template_engine: TemplateEngine,
}

impl DynamicPromptExecutor {
    /// Create a new dynamic prompt executor
    pub fn new(max_entries: Option<usize>) -> Self {
        Self {
            schema_metrics: Arc::new(RwLock::new(HashMap::new())),
            model_metrics: Arc::new(RwLock::new(HashMap::new())),
            max_entries: max_entries.unwrap_or(1000),
            template_engine: TemplateEngine::new(),
        }
    }

    /// Execute a prompt with schema validation
    #[instrument(skip(self, state, schema, prompt_template, model_fn), level = "info")]
    pub async fn execute_from_state<F, Fut>(
        &self,
        state: &State,
        schema: Vec<SchemaRow>,
        prompt_template: &str,
        options: DynamicPromptOptions,
        model_fn: F,
    ) -> Result<HashMap<String, serde_json::Value>>
    where
        F: Fn(String, DynamicPromptOptions) -> Fut,
        Fut: std::future::Future<Output = Result<String>>,
    {
        let start_time = Instant::now();

        // Generate schema key
        let schema_key = self.generate_schema_key(&schema);

        // Generate cache key
        let model_identifier =
            options
                .model
                .clone()
                .unwrap_or_else(|| match options.model_size.as_deref() {
                    Some("small") => "TEXT_SMALL".to_string(),
                    _ => "TEXT_LARGE".to_string(),
                });

        let cache_key = options
            .key
            .clone()
            .unwrap_or_else(|| self.generate_cache_key(state, &schema, &model_identifier));

        debug!(
            "Executing dynamic prompt: schema_key={}, cache_key={}",
            schema_key, cache_key
        );

        // Generate validation codes for retry detection
        let validation_codes = self.generate_validation_codes();

        // Compose prompt from state
        let mut template_data: HashMap<String, serde_json::Value> = HashMap::new();

        // Add state values
        for (key, value) in &state.values {
            template_data.insert(key.clone(), serde_json::Value::String(value.clone()));
        }

        // Add state data
        for (key, value) in &state.data {
            template_data.insert(key.clone(), value.clone());
        }

        // Add schema to template data
        template_data.insert("schema".to_string(), serde_json::to_value(&schema)?);

        // Add validation codes
        template_data.insert(
            "validationCodes".to_string(),
            serde_json::to_value(&validation_codes)?,
        );

        // Render prompt
        let prompt = self
            .template_engine
            .render(prompt_template, &template_data)?;

        // Estimate tokens
        let estimated_tokens = estimate_tokens(&prompt);
        debug!("Estimated tokens: {}", estimated_tokens);

        // Execute with retries
        let mut retry_count = 0;
        let mut last_error: Option<String> = None;

        while retry_count <= options.max_retries {
            // Call model
            let response = match model_fn(prompt.clone(), options.clone()).await {
                Ok(r) => r,
                Err(e) => {
                    warn!("Model call failed (attempt {}): {}", retry_count + 1, e);
                    last_error = Some(e.to_string());
                    retry_count += 1;
                    continue;
                }
            };

            // Parse response based on format
            let parsed = match options.force_format {
                Some(ResponseFormat::Xml) => self.parse_xml_response(&response, &schema),
                Some(ResponseFormat::Json) => self.parse_json_response(&response, &schema),
                None => {
                    // Auto-detect format
                    if response.trim().starts_with('{') || response.trim().starts_with('[') {
                        self.parse_json_response(&response, &schema)
                    } else {
                        self.parse_xml_response(&response, &schema)
                    }
                }
            };

            match parsed {
                Ok(result) => {
                    // Validate against schema
                    let validation_result =
                        self.validate_response(&result, &schema, &options.validation_level);

                    if validation_result.is_ok() {
                        // Success!
                        let duration = start_time.elapsed();

                        // Record metrics
                        self.record_schema_metrics(&schema_key, true, retry_count, duration);
                        self.record_model_metrics(
                            &model_identifier,
                            estimated_tokens,
                            duration,
                            true,
                        );

                        info!(
                            "Dynamic prompt execution succeeded (retries: {}, {}ms)",
                            retry_count,
                            duration.as_millis()
                        );

                        return Ok(result);
                    } else {
                        warn!(
                            "Validation failed (attempt {}): {:?}",
                            retry_count + 1,
                            validation_result
                        );
                        last_error = Some(format!("{:?}", validation_result));
                    }
                }
                Err(e) => {
                    warn!("Parse failed (attempt {}): {}", retry_count + 1, e);
                    last_error = Some(e.to_string());
                }
            }

            retry_count += 1;
        }

        // All retries exhausted
        let duration = start_time.elapsed();
        self.record_schema_metrics(&schema_key, false, retry_count, duration);
        self.record_model_metrics(&model_identifier, estimated_tokens, duration, false);

        error!(
            "Dynamic prompt execution failed after {} retries",
            retry_count
        );
        Err(ZoeyError::Validation(
            last_error.unwrap_or_else(|| "All retries exhausted".to_string()),
        ))
    }

    /// Generate schema key for metrics
    fn generate_schema_key(&self, schema: &[SchemaRow]) -> String {
        use sha2::{Digest, Sha256};

        let schema_str = schema
            .iter()
            .map(|s| format!("{}:{}", s.field, s.field_type as u8))
            .collect::<Vec<_>>()
            .join(",");

        let mut hasher = Sha256::new();
        hasher.update(schema_str.as_bytes());
        let hash = hasher.finalize();

        format!(
            "schema_{:x}",
            &hash[..8].iter().fold(0u64, |acc, &b| (acc << 8) | b as u64)
        )
    }

    /// Generate cache key
    fn generate_cache_key(&self, state: &State, schema: &[SchemaRow], model: &str) -> String {
        use sha2::{Digest, Sha256};

        let state_str = format!("{:?}", state.values);
        let schema_str = format!("{:?}", schema);
        let combined = format!("{}:{}:{}", state_str, schema_str, model);

        let mut hasher = Sha256::new();
        hasher.update(combined.as_bytes());
        let hash = hasher.finalize();

        format!(
            "cache_{:x}",
            &hash[..8].iter().fold(0u64, |acc, &b| (acc << 8) | b as u64)
        )
    }

    /// Generate validation codes for retry detection
    fn generate_validation_codes(&self) -> HashMap<String, String> {
        use uuid::Uuid;

        let init_code = Uuid::new_v4().to_string();
        let mid_code = Uuid::new_v4().to_string();
        let final_code = Uuid::new_v4().to_string();

        let mut codes = HashMap::new();

        // First validation checkpoint
        codes.insert("one_initial_code".to_string(), init_code.clone());
        codes.insert("one_middle_code".to_string(), mid_code.clone());
        codes.insert("one_end_code".to_string(), final_code.clone());

        // Second validation checkpoint
        codes.insert("two_initial_code".to_string(), init_code);
        codes.insert("two_middle_code".to_string(), mid_code);
        codes.insert("two_end_code".to_string(), final_code);

        codes
    }

    /// Parse XML response
    fn parse_xml_response(
        &self,
        response: &str,
        schema: &[SchemaRow],
    ) -> Result<HashMap<String, serde_json::Value>> {
        let mut result = HashMap::new();

        for field in schema {
            if let Some(value) = extract_xml_tag(response, &field.field) {
                let parsed_value = match field.field_type {
                    SchemaType::String => serde_json::Value::String(value),
                    SchemaType::Number => value
                        .parse::<f64>()
                        .map(serde_json::Value::from)
                        .unwrap_or_else(|_| serde_json::Value::String(value)),
                    SchemaType::Boolean => serde_json::Value::Bool(value.to_lowercase() == "true"),
                    SchemaType::Uuid => {
                        // Validate UUID format
                        if uuid::Uuid::parse_str(&value).is_ok() {
                            serde_json::Value::String(value)
                        } else {
                            serde_json::Value::String(value)
                        }
                    }
                    SchemaType::Array => {
                        // Try to parse as JSON array
                        serde_json::from_str(&value)
                            .unwrap_or_else(|_| serde_json::Value::String(value))
                    }
                    SchemaType::Object => serde_json::from_str(&value)
                        .unwrap_or_else(|_| serde_json::Value::String(value)),
                    SchemaType::Enum => serde_json::Value::String(value),
                };

                result.insert(field.field.clone(), parsed_value);
            } else if field.required {
                warn!("Required field '{}' not found in XML response", field.field);
            }
        }

        Ok(result)
    }

    /// Parse JSON response
    fn parse_json_response(
        &self,
        response: &str,
        schema: &[SchemaRow],
    ) -> Result<HashMap<String, serde_json::Value>> {
        let json: serde_json::Value = serde_json::from_str(response.trim())?;

        let mut result = HashMap::new();

        if let Some(obj) = json.as_object() {
            for field in schema {
                if let Some(value) = obj.get(&field.field) {
                    result.insert(field.field.clone(), value.clone());
                } else if field.required {
                    warn!(
                        "Required field '{}' not found in JSON response",
                        field.field
                    );
                }
            }
        }

        Ok(result)
    }

    /// Validate response against schema
    fn validate_response(
        &self,
        response: &HashMap<String, serde_json::Value>,
        schema: &[SchemaRow],
        validation_level: &ValidationLevel,
    ) -> Result<()> {
        if *validation_level == ValidationLevel::None {
            return Ok(());
        }

        for field in schema {
            // Check required fields
            if field.required && !response.contains_key(&field.field) {
                return Err(ZoeyError::Validation(format!(
                    "Required field '{}' is missing",
                    field.field
                )));
            }

            if let Some(value) = response.get(&field.field) {
                // Type validation
                if *validation_level >= ValidationLevel::Basic {
                    let type_valid = match field.field_type {
                        SchemaType::String => value.is_string(),
                        SchemaType::Number => value.is_number(),
                        SchemaType::Boolean => value.is_boolean(),
                        SchemaType::Array => value.is_array(),
                        SchemaType::Object => value.is_object(),
                        SchemaType::Uuid => {
                            value.is_string()
                                && value
                                    .as_str()
                                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                                    .is_some()
                        }
                        SchemaType::Enum => value.is_string(),
                    };

                    if !type_valid {
                        return Err(ZoeyError::Validation(format!(
                            "Field '{}' has incorrect type (expected {:?})",
                            field.field, field.field_type
                        )));
                    }
                }

                // Regex validation
                if *validation_level >= ValidationLevel::Maximum {
                    if let Some(ref regex_pattern) = field.validation {
                        if let Some(str_value) = value.as_str() {
                            let regex = regex::Regex::new(regex_pattern).map_err(|e| {
                                ZoeyError::Validation(format!("Invalid regex pattern: {}", e))
                            })?;
                            if !regex.is_match(str_value) {
                                return Err(ZoeyError::Validation(format!(
                                    "Field '{}' failed regex validation",
                                    field.field
                                )));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Record schema execution metrics
    fn record_schema_metrics(
        &self,
        schema_key: &str,
        success: bool,
        retries: usize,
        duration: Duration,
    ) {
        let mut metrics = self.schema_metrics.write().unwrap();

        // LRU eviction if needed
        if metrics.len() >= self.max_entries {
            if let Some(oldest_key) = metrics
                .iter()
                .min_by_key(|(_, m)| m.last_execution)
                .map(|(k, _)| k.clone())
            {
                metrics.remove(&oldest_key);
                debug!("Evicted oldest schema metrics entry");
            }
        }

        let entry = metrics
            .entry(schema_key.to_string())
            .or_insert_with(|| SchemaMetrics::new(schema_key.to_string()));

        entry.record_execution(success, retries, duration.as_millis());
    }

    /// Record model execution metrics
    fn record_model_metrics(&self, model: &str, tokens: usize, duration: Duration, success: bool) {
        let mut metrics = self.model_metrics.write().unwrap();

        let entry = metrics
            .entry(model.to_string())
            .or_insert_with(|| ModelMetrics::new(model.to_string()));

        entry.record_call(tokens, duration.as_millis(), success);
    }

    /// Get schema metrics
    pub fn get_schema_metrics(&self) -> HashMap<String, SchemaMetrics> {
        self.schema_metrics.read().unwrap().clone()
    }

    /// Get model metrics
    pub fn get_model_metrics(&self) -> HashMap<String, ModelMetrics> {
        self.model_metrics.read().unwrap().clone()
    }

    /// Clear all metrics
    pub fn clear_metrics(&self) {
        self.schema_metrics.write().unwrap().clear();
        self.model_metrics.write().unwrap().clear();
        info!("Cleared all dynamic prompt metrics");
    }

    /// Get metrics summary
    pub fn get_metrics_summary(&self) -> MetricsSummary {
        let schema_metrics = self.schema_metrics.read().unwrap();
        let model_metrics = self.model_metrics.read().unwrap();

        let total_executions: usize = schema_metrics.values().map(|m| m.execution_count).sum();

        let total_successes: usize = schema_metrics.values().map(|m| m.success_count).sum();

        let total_failures: usize = schema_metrics.values().map(|m| m.failure_count).sum();

        let total_retries: usize = schema_metrics.values().map(|m| m.retry_count).sum();

        let avg_response_time: f64 = if !schema_metrics.is_empty() {
            schema_metrics
                .values()
                .map(|m| m.avg_response_time_ms)
                .sum::<f64>()
                / schema_metrics.len() as f64
        } else {
            0.0
        };

        let total_tokens: usize = model_metrics.values().map(|m| m.total_tokens).sum();

        MetricsSummary {
            total_executions,
            total_successes,
            total_failures,
            total_retries,
            success_rate: if total_executions > 0 {
                total_successes as f64 / total_executions as f64
            } else {
                0.0
            },
            avg_response_time_ms: avg_response_time,
            total_tokens,
            unique_schemas: schema_metrics.len(),
            unique_models: model_metrics.len(),
        }
    }
}

impl Default for DynamicPromptExecutor {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSummary {
    /// Total number of executions
    pub total_executions: usize,
    /// Total successful executions
    pub total_successes: usize,
    /// Total failed executions
    pub total_failures: usize,
    /// Total retry attempts across all executions
    pub total_retries: usize,
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Total tokens processed
    pub total_tokens: usize,
    /// Number of unique schemas used
    pub unique_schemas: usize,
    /// Number of unique models used
    pub unique_models: usize,
}

/// Extract XML tag content
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start_pos) = xml.find(&start_tag) {
        let content_start = start_pos + start_tag.len();
        if let Some(end_pos) = xml[content_start..].find(&end_tag) {
            return Some(
                xml[content_start..content_start + end_pos]
                    .trim()
                    .to_string(),
            );
        }
    }
    None
}

/// Estimate token count for prompt (rough approximation)
fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: ~4 characters per token
    (text.len() as f64 / 4.0).ceil() as usize
}

/// Upgrade double quotes to triple quotes for code blocks
pub fn upgrade_double_to_triple(text: &str) -> String {
    text.replace("``", "```")
}

/// Compose random user name for examples
pub fn compose_random_user() -> String {
    use names::Generator;
    let mut generator = Generator::default();
    generator.next().unwrap_or_else(|| "User".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_row() {
        let row = SchemaRow {
            field: "response".to_string(),
            description: "The agent's response".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: Some("Hello!".to_string()),
            validation: None,
        };

        assert_eq!(row.field, "response");
        assert!(row.required);
    }

    #[test]
    fn test_validation_level() {
        assert_eq!(ValidationLevel::default(), ValidationLevel::Strict);
        assert!(ValidationLevel::Maximum >= ValidationLevel::Strict);
    }

    #[test]
    fn test_estimate_tokens() {
        let text = "Hello world";
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
        assert!(tokens < 10);
    }

    #[test]
    fn test_extract_xml_tag() {
        let xml = "<response><text>Hello</text><thought>Thinking</thought></response>";

        assert_eq!(extract_xml_tag(xml, "text"), Some("Hello".to_string()));
        assert_eq!(
            extract_xml_tag(xml, "thought"),
            Some("Thinking".to_string())
        );
        assert_eq!(extract_xml_tag(xml, "missing"), None);
    }

    #[test]
    fn test_upgrade_double_to_triple() {
        let text = "Here is code: ``python\nprint('hi')\n``";
        let upgraded = upgrade_double_to_triple(text);
        assert!(upgraded.contains("```python"));
    }

    #[test]
    fn test_compose_random_user() {
        let user = compose_random_user();
        assert!(!user.is_empty());
    }

    #[tokio::test]
    async fn test_dynamic_prompt_executor() {
        let executor = DynamicPromptExecutor::new(Some(100));

        let schema = vec![SchemaRow {
            field: "response".to_string(),
            description: "Response text".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: None,
            validation: None,
        }];

        let state = State::new();

        // Mock model function
        let model_fn = |_prompt: String, _opts: DynamicPromptOptions| async {
            Ok("<response>Test response</response>".to_string())
        };

        let result = executor
            .execute_from_state(
                &state,
                schema,
                "Generate response",
                DynamicPromptOptions::default(),
                model_fn,
            )
            .await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_metrics_summary() {
        let executor = DynamicPromptExecutor::new(None);
        let summary = executor.get_metrics_summary();

        assert_eq!(summary.total_executions, 0);
        assert_eq!(summary.success_rate, 0.0);
    }

    #[tokio::test]
    async fn test_validation_failure_retry() {
        let executor = DynamicPromptExecutor::new(None);

        let schema = vec![SchemaRow {
            field: "required_field".to_string(),
            description: "A required field".to_string(),
            field_type: SchemaType::String,
            required: true,
            example: None,
            validation: None,
        }];

        let state = State::new();

        // Model always returns incomplete response
        let model_fn = |_prompt: String, _opts: DynamicPromptOptions| async {
            Ok("<response></response>".to_string())
        };

        let mut opts = DynamicPromptOptions::default();
        opts.max_retries = 2;

        let result = executor
            .execute_from_state(&state, schema, "Test", opts, model_fn)
            .await;

        // Should fail after retries
        assert!(result.is_err());

        // Check metrics
        let summary = executor.get_metrics_summary();
        assert_eq!(summary.total_failures, 1);
        assert!(summary.total_retries >= 2);
    }
}
