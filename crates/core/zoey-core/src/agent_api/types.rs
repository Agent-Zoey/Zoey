//! Types for Agent API
//!
//! Defines request and response structures for agent endpoint communication

use crate::types::{Content, Memory, State, UUID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request to create a memory (for async memory persistence)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCreateRequest {
    /// Room ID
    #[serde(alias = "room_id")]
    pub room_id: UUID,

    /// Entity ID (who created this memory)
    #[serde(alias = "entity_id")]
    pub entity_id: UUID,

    /// Message text content
    pub text: String,

    /// Source platform (discord, telegram, web, etc.)
    #[serde(default = "default_source")]
    pub source: String,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Response from memory creation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCreateResponse {
    /// Success status
    pub success: bool,

    /// Created memory ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<UUID>,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Request to send a message to the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    /// Message text
    pub text: String,

    /// Room ID (conversation context)
    #[serde(alias = "room_id")]
    pub room_id: UUID,

    /// Entity ID (user/sender)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "entity_id")]
    pub entity_id: Option<UUID>,

    /// Source platform (e.g., "web", "mobile", "api")
    #[serde(default = "default_source")]
    pub source: String,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,

    /// Whether to stream the response
    #[serde(default)]
    pub stream: bool,
}

fn default_source() -> String {
    "api".to_string()
}

/// Response from chat endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    /// Success status
    pub success: bool,

    /// Generated memories (messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Memory>>,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Response metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Request to execute an action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRequest {
    /// Action name
    pub action: String,

    /// Room ID
    #[serde(alias = "room_id")]
    pub room_id: UUID,

    /// Entity ID
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "entity_id")]
    pub entity_id: Option<UUID>,

    /// Action parameters
    #[serde(default)]
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Response from action execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResponse {
    /// Success status
    pub success: bool,

    /// Action result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Request to get agent state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateRequest {
    /// Room ID
    #[serde(alias = "room_id")]
    pub room_id: UUID,

    /// Entity ID
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "entity_id")]
    pub entity_id: Option<UUID>,
}

/// Response with agent state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateResponse {
    /// Success status
    pub success: bool,

    /// Agent state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<State>,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Status (ok/error)
    pub status: String,

    /// Agent ID
    pub agent_id: UUID,

    /// Agent name
    pub agent_name: String,

    /// Uptime in seconds
    pub uptime: u64,

    /// Timestamp
    pub timestamp: String,
}

/// Generic API response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    /// Success status
    pub success: bool,

    /// Response data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,

    /// Error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// HTTP status code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u16>,
}

impl<T> ApiResponse<T> {
    /// Create a success response
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            code: Some(200),
        }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>, code: u16) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
            code: Some(code),
        }
    }
}

/// Server-Sent Event for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEvent {
    /// Event type
    pub event: String,

    /// Event data
    pub data: serde_json::Value,
}

/// Permission levels for API access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiPermission {
    /// Read-only access
    Read,

    /// Write access (send messages)
    Write,

    /// Execute actions
    Execute,

    /// Admin access
    Admin,
}

/// Authentication token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    /// Token identifier (hashed)
    pub token: String,

    /// Token name/description
    pub name: String,

    /// Permissions granted to this token
    pub permissions: Vec<ApiPermission>,

    /// Expiration timestamp (Unix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,

    /// Agent ID this token is scoped to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<UUID>,
}

/// Task submission response (returned immediately)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    /// Success status
    pub success: bool,

    /// Task ID for polling
    pub task_id: UUID,

    /// Message about task submission
    pub message: String,

    /// Estimated time in milliseconds (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_time_ms: Option<u64>,
}

/// Task status response (returned when polling)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusResponse {
    /// Task ID
    pub task_id: UUID,

    /// Task status
    pub status: String,

    /// Result if completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    /// Error if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Task duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,

    /// Timestamp when task was created
    pub created_at: String,

    /// Timestamp when task completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

// ============================================================================
// Knowledge Ingestion Types
// ============================================================================

/// Supported document types for knowledge ingestion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeDocumentType {
    /// Plain text
    Text,
    /// Markdown format
    Markdown,
    /// CSV data
    Csv,
    /// JSON data
    Json,
    /// PDF document
    Pdf,
    /// Excel spreadsheet (xlsx, xls)
    Excel,
}

impl KnowledgeDocumentType {
    /// Get allowed file extensions for this type
    pub fn allowed_extensions(&self) -> &[&str] {
        match self {
            Self::Text => &["txt"],
            Self::Markdown => &["md", "markdown"],
            Self::Csv => &["csv"],
            Self::Json => &["json"],
            Self::Pdf => &["pdf"],
            Self::Excel => &["xlsx", "xls"],
        }
    }

    /// Infer document type from filename
    pub fn from_filename(filename: &str) -> Option<Self> {
        let ext = filename.rsplit('.').next()?.to_lowercase();
        match ext.as_str() {
            "txt" => Some(Self::Text),
            "md" | "markdown" => Some(Self::Markdown),
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            "pdf" => Some(Self::Pdf),
            "xlsx" | "xls" => Some(Self::Excel),
            _ => None,
        }
    }

    /// Check if a MIME type is valid for this document type
    pub fn valid_mime_type(&self, mime: &str) -> bool {
        let mime_lower = mime.to_lowercase();
        match self {
            Self::Text => {
                mime_lower.starts_with("text/plain")
                    || mime_lower.starts_with("application/octet-stream")
            }
            Self::Markdown => {
                mime_lower.starts_with("text/markdown")
                    || mime_lower.starts_with("text/plain")
                    || mime_lower.starts_with("text/x-markdown")
                    || mime_lower.starts_with("application/octet-stream")
            }
            Self::Csv => {
                mime_lower.starts_with("text/csv")
                    || mime_lower.starts_with("text/plain")
                    || mime_lower.starts_with("application/csv")
                    || mime_lower.starts_with("application/octet-stream")
            }
            Self::Json => {
                mime_lower.starts_with("application/json")
                    || mime_lower.starts_with("text/json")
                    || mime_lower.starts_with("text/plain")
                    || mime_lower.starts_with("application/octet-stream")
            }
            Self::Pdf => {
                mime_lower.starts_with("application/pdf")
                    || mime_lower.starts_with("application/octet-stream")
            }
            Self::Excel => {
                mime_lower.starts_with("application/vnd.openxmlformats-officedocument.spreadsheetml")
                    || mime_lower.starts_with("application/vnd.ms-excel")
                    || mime_lower.starts_with("application/excel")
                    || mime_lower.starts_with("application/octet-stream")
            }
        }
    }

    /// Check if this document type requires base64 encoding (binary formats)
    pub fn requires_base64(&self) -> bool {
        matches!(self, Self::Pdf | Self::Excel)
    }
}

/// Request to ingest a document into the knowledge system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeIngestRequest {
    /// Room ID (case/conversation context)
    #[serde(alias = "room_id")]
    pub room_id: UUID,

    /// Entity ID (who is uploading)
    #[serde(alias = "entity_id")]
    pub entity_id: UUID,

    /// Document filename (used to infer type and for logging)
    pub filename: String,

    /// Document content (plain text or base64 encoded)
    pub content: String,

    /// Whether content is base64 encoded
    #[serde(default)]
    pub base64_encoded: bool,

    /// Optional explicit document type (inferred from filename if not provided)
    #[serde(alias = "doc_type")]
    pub document_type: Option<KnowledgeDocumentType>,

    /// Optional MIME type for validation
    #[serde(alias = "mime_type")]
    pub mime_type: Option<String>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Response from knowledge ingestion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeIngestResponse {
    /// Success status
    pub success: bool,

    /// Created document ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<UUID>,

    /// Number of chunks created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunks_created: Option<usize>,

    /// Word count of ingested content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_count: Option<usize>,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Warnings (e.g., content truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

impl KnowledgeIngestResponse {
    /// Create a success response
    pub fn success(document_id: UUID, chunks_created: usize, word_count: usize) -> Self {
        Self {
            success: true,
            document_id: Some(document_id),
            chunks_created: Some(chunks_created),
            word_count: Some(word_count),
            error: None,
            warnings: None,
        }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            document_id: None,
            chunks_created: None,
            word_count: None,
            error: Some(message.into()),
            warnings: None,
        }
    }

    /// Add warnings
    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = if warnings.is_empty() {
            None
        } else {
            Some(warnings)
        };
        self
    }
}

/// Query request for knowledge retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQueryRequest {
    /// Room ID to search within
    #[serde(alias = "room_id")]
    pub room_id: UUID,

    /// Search query
    pub query: String,

    /// Maximum results to return
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    10
}

/// A knowledge chunk result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunkResult {
    /// Chunk ID
    pub id: UUID,

    /// Source document ID
    pub document_id: UUID,

    /// Chunk text
    pub text: String,

    /// Relevance score (0.0 - 1.0)
    pub score: f64,

    /// Source filename
    pub filename: Option<String>,
}

/// Response from knowledge query
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQueryResponse {
    /// Success status
    pub success: bool,

    /// Search results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<KnowledgeChunkResult>>,

    /// Total documents in knowledge base
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_documents: Option<usize>,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ============================================================================
// Provider Management Types
// ============================================================================

/// Request to switch model provider
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSwitchRequest {
    /// Provider name to switch to (must be a registered provider)
    pub provider: String,
}

/// Response listing available providers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersListResponse {
    /// Success status
    pub success: bool,

    /// List of available provider names
    pub providers: Vec<String>,

    /// Currently selected provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

/// Response from provider switch
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSwitchResponse {
    /// Success status
    pub success: bool,

    /// The provider that was switched to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_chat_request() {
        let req = ChatRequest {
            text: "Hello".to_string(),
            room_id: Uuid::new_v4(),
            entity_id: None,
            source: "test".to_string(),
            metadata: HashMap::new(),
            stream: false,
        };
        assert_eq!(req.text, "Hello");
    }

    #[test]
    fn test_api_response() {
        let response = ApiResponse::success("test data");
        assert!(response.success);
        assert_eq!(response.data, Some("test data"));

        let error: ApiResponse<String> = ApiResponse::error("test error", 400);
        assert!(!error.success);
        assert_eq!(error.error, Some("test error".to_string()));
    }
}
