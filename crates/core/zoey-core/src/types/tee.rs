//! TEE (Trusted Execution Environment) types

use super::primitives::Metadata;
use serde::{Deserialize, Serialize};

/// Represents an agent's registration details within a Trusted Execution Environment (TEE) context.
/// This is typically stored in a database table (e.g., `TeeAgent`) to manage agents operating in a TEE.
/// It allows for multiple registrations of the same `agent_id` to support scenarios where an agent might restart,
/// generating a new keypair and attestation each time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeeAgent {
    /// Primary key for the TEE agent registration record (e.g., a UUID or auto-incrementing ID).
    pub id: String,

    /// The core identifier of the agent, which can be duplicated across multiple TEE registrations.
    pub agent_id: String,

    /// The human-readable name of the agent.
    pub agent_name: String,

    /// Timestamp (e.g., Unix epoch in milliseconds) when this TEE registration was created.
    pub created_at: i64,

    /// The public key associated with this specific TEE agent instance/session.
    pub public_key: String,

    /// The attestation document proving the authenticity and integrity of the TEE instance.
    pub attestation: String,
}

/// Defines the operational modes for a Trusted Execution Environment (TEE).
/// This enum is used to configure how TEE functionalities are engaged, allowing for
/// different setups for local development, Docker-based development, and production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TeeMode {
    /// TEE functionality is completely disabled.
    Off,

    /// For local development, potentially using a TEE simulator.
    Local,

    /// For Docker-based development environments, possibly with a TEE simulator.
    Docker,

    /// For production deployments, using actual TEE hardware without a simulator.
    Production,
}

/// Represents a quote obtained during remote attestation for a Trusted Execution Environment (TEE).
/// This quote is a piece of evidence provided by the TEE, cryptographically signed, which can be
/// verified by a relying party to ensure the TEE's integrity and authenticity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAttestationQuote {
    /// The attestation quote data, typically a base64 encoded string or similar format.
    pub quote: String,

    /// Timestamp (e.g., Unix epoch in milliseconds) when the quote was generated or received.
    pub timestamp: i64,
}

/// Data structure used in the attestation process for deriving a key within a Trusted Execution Environment (TEE).
/// This information helps establish a secure channel or verify the identity of the agent instance
/// requesting key derivation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeriveKeyAttestationData {
    /// The unique identifier of the agent for which the key derivation is being attested.
    pub agent_id: String,

    /// The public key of the agent instance involved in the key derivation process.
    pub public_key: String,

    /// Optional subject or context information related to the key derivation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

/// Represents a message that has been attested by a Trusted Execution Environment (TEE).
/// This structure binds a message to an agent's identity and a timestamp, all within the
/// context of a remote attestation process, ensuring the message originated from a trusted TEE instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAttestationMessage {
    /// The unique identifier of the agent sending the attested message.
    pub agent_id: String,

    /// Timestamp (e.g., Unix epoch in milliseconds) when the message was attested or sent.
    pub timestamp: i64,

    /// The actual message content, including details about the entity, room, and the content itself.
    pub message: RemoteAttestationMessageContent,
}

/// Content of a remote attestation message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAttestationMessageContent {
    /// Entity ID
    pub entity_id: String,

    /// Room ID
    pub room_id: String,

    /// Message content
    pub content: String,
}

/// Enumerates different types or vendors of Trusted Execution Environments (TEEs).
/// This allows the system to adapt to specific TEE technologies, like Intel TDX on DSTACK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeeType {
    /// Represents Intel Trusted Domain Extensions (TDX) running on DSTACK infrastructure.
    #[serde(rename = "tdx_dstack")]
    TdxDstack,
}

/// Configuration for a TEE (Trusted Execution Environment) plugin.
/// This allows specifying the TEE vendor and any vendor-specific configurations.
/// It's used to initialize and configure TEE-related functionalities within the agent system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeePluginConfig {
    /// Optional. The name or identifier of the TEE vendor (e.g., 'tdx_dstack' from `TeeType`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,

    /// Optional. Vendor-specific configuration options, conforming to `Metadata`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_config: Option<Metadata>,
}

impl Default for TeeMode {
    fn default() -> Self {
        TeeMode::Off
    }
}

impl TeePluginConfig {
    /// Create a new TEE plugin config
    pub fn new() -> Self {
        Self {
            vendor: None,
            vendor_config: None,
        }
    }

    /// Set the vendor
    pub fn with_vendor(mut self, vendor: String) -> Self {
        self.vendor = Some(vendor);
        self
    }

    /// Set the vendor config
    pub fn with_vendor_config(mut self, config: Metadata) -> Self {
        self.vendor_config = Some(config);
        self
    }
}

impl Default for TeePluginConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tee_mode() {
        assert_eq!(TeeMode::default(), TeeMode::Off);
    }

    #[test]
    fn test_tee_plugin_config() {
        let config = TeePluginConfig::new().with_vendor("tdx_dstack".to_string());

        assert_eq!(config.vendor, Some("tdx_dstack".to_string()));
    }
}
