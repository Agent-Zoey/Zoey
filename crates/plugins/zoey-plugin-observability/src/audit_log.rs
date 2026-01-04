/*!
# Tamper-Evident Audit Log Module

Provides cryptographically secure audit trails for compliance (HIPAA, GDPR, etc.)
*/

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use uuid::Uuid;

/// A single audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique identifier
    pub id: Uuid,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// The explainability context (JSON serialized)
    pub context_json: String,

    /// Cryptographic hash of this entry + previous hash (blockchain-style)
    pub hash: String,
}

/// Tamper-evident audit log using hash chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TamperEvidentLog {
    /// All entries in the log
    entries: Vec<AuditEntry>,

    /// Hash of the previous entry (for chain verification)
    previous_hash: String,

    /// Metadata
    metadata: LogMetadata,
}

/// Metadata about the audit log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMetadata {
    /// When the log was created
    pub created_at: DateTime<Utc>,

    /// Version of the log format
    pub version: String,

    /// System identifier
    pub system_id: String,

    /// Agent identifier
    pub agent_id: Option<Uuid>,
}

impl TamperEvidentLog {
    /// Create a new tamper-evident log
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            previous_hash: Self::genesis_hash(),
            metadata: LogMetadata {
                created_at: Utc::now(),
                version: "1.0".to_string(),
                system_id: "laura-ai".to_string(),
                agent_id: None,
            },
        }
    }

    /// Create a log with agent ID
    pub fn with_agent_id(agent_id: Uuid) -> Self {
        let mut log = Self::new();
        log.metadata.agent_id = Some(agent_id);
        log
    }

    /// Genesis hash (first hash in the chain)
    fn genesis_hash() -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"LAURA_AI_AUDIT_LOG_GENESIS");
        format!("{:x}", hasher.finalize())
    }

    /// Calculate hash for an entry
    fn calculate_hash(entry: &AuditEntry, previous_hash: &str) -> String {
        let mut hasher = Sha256::new();

        // Hash: previous_hash + timestamp + id + context
        hasher.update(previous_hash.as_bytes());
        hasher.update(entry.timestamp.to_rfc3339().as_bytes());
        hasher.update(entry.id.as_bytes());
        hasher.update(entry.context_json.as_bytes());

        format!("{:x}", hasher.finalize())
    }

    /// Append a new entry to the log
    pub fn append(&mut self, mut entry: AuditEntry) -> anyhow::Result<()> {
        // Calculate hash based on previous hash
        let hash = Self::calculate_hash(&entry, &self.previous_hash);
        entry.hash = hash.clone();

        self.entries.push(entry);
        self.previous_hash = hash;

        Ok(())
    }

    /// Verify the integrity of the entire log
    pub fn verify(&self) -> anyhow::Result<bool> {
        let mut previous_hash = Self::genesis_hash();

        for entry in &self.entries {
            let expected_hash = Self::calculate_hash(entry, &previous_hash);

            if entry.hash != expected_hash {
                return Ok(false); // Tampering detected
            }

            previous_hash = entry.hash.clone();
        }

        Ok(true)
    }

    /// Get all entries
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    /// Get entry by ID
    pub fn get_entry(&self, id: &Uuid) -> Option<&AuditEntry> {
        self.entries.iter().find(|e| &e.id == id)
    }

    /// Get entries in time range
    pub fn entries_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect()
    }

    /// Export to JSON for compliance reporting
    pub fn export_json(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| e.into())
    }

    /// Export entries to CSV
    pub fn export_csv(&self) -> String {
        let mut csv = String::from("ID,Timestamp,Hash,Context\n");

        for entry in &self.entries {
            // Escape context JSON for CSV
            let context = entry.context_json.replace('"', "\"\"");
            csv.push_str(&format!(
                "{},{},{},\"{}\"\n",
                entry.id,
                entry.timestamp.to_rfc3339(),
                entry.hash,
                context
            ));
        }

        csv
    }

    /// Get statistics about the log
    pub fn statistics(&self) -> LogStatistics {
        LogStatistics {
            total_entries: self.entries.len(),
            earliest_entry: self.entries.first().map(|e| e.timestamp),
            latest_entry: self.entries.last().map(|e| e.timestamp),
            verified: self.verify().unwrap_or(false),
        }
    }
}

impl Default for TamperEvidentLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about an audit log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStatistics {
    /// Total number of entries
    pub total_entries: usize,

    /// Timestamp of earliest entry
    pub earliest_entry: Option<DateTime<Utc>>,

    /// Timestamp of latest entry
    pub latest_entry: Option<DateTime<Utc>>,

    /// Whether the log passes verification
    pub verified: bool,
}

impl fmt::Display for LogStatistics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Audit Log Statistics:")?;
        writeln!(f, "  Total Entries: {}", self.total_entries)?;
        writeln!(
            f,
            "  Earliest: {}",
            self.earliest_entry
                .map(|d| d.to_rfc3339())
                .unwrap_or_else(|| "N/A".to_string())
        )?;
        writeln!(
            f,
            "  Latest: {}",
            self.latest_entry
                .map(|d| d.to_rfc3339())
                .unwrap_or_else(|| "N/A".to_string())
        )?;
        writeln!(
            f,
            "  Integrity: {}",
            if self.verified {
                "✓ Verified"
            } else {
                "✗ TAMPERED"
            }
        )?;
        Ok(())
    }
}

/// Audit log manager for multiple agents
pub struct AuditLogManager {
    logs: std::collections::HashMap<Uuid, TamperEvidentLog>,
}

impl AuditLogManager {
    /// Create a new manager
    pub fn new() -> Self {
        Self {
            logs: std::collections::HashMap::new(),
        }
    }

    /// Get or create a log for an agent
    pub fn get_or_create_log(&mut self, agent_id: Uuid) -> &mut TamperEvidentLog {
        self.logs
            .entry(agent_id)
            .or_insert_with(|| TamperEvidentLog::with_agent_id(agent_id))
    }

    /// Verify all logs
    pub fn verify_all(&self) -> anyhow::Result<bool> {
        for log in self.logs.values() {
            if !log.verify()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Get statistics for all logs
    pub fn all_statistics(&self) -> Vec<(Uuid, LogStatistics)> {
        self.logs
            .iter()
            .map(|(id, log)| (*id, log.statistics()))
            .collect()
    }
}

impl Default for AuditLogManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_creation() {
        let log = TamperEvidentLog::new();
        assert_eq!(log.entries().len(), 0);
        assert!(log.verify().unwrap());
    }

    #[test]
    fn test_append_and_verify() {
        let mut log = TamperEvidentLog::new();

        let entry1 = AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            context_json: r#"{"test": "data1"}"#.to_string(),
            hash: String::new(),
        };

        let entry2 = AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            context_json: r#"{"test": "data2"}"#.to_string(),
            hash: String::new(),
        };

        log.append(entry1).unwrap();
        log.append(entry2).unwrap();

        assert_eq!(log.entries().len(), 2);
        assert!(log.verify().unwrap());
    }

    #[test]
    fn test_tampering_detection() {
        let mut log = TamperEvidentLog::new();

        let entry = AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            context_json: r#"{"test": "data"}"#.to_string(),
            hash: String::new(),
        };

        log.append(entry).unwrap();

        // Tamper with the entry
        log.entries[0].context_json = r#"{"test": "tampered"}"#.to_string();

        // Verification should fail
        assert!(!log.verify().unwrap());
    }

    #[test]
    fn test_export() {
        let mut log = TamperEvidentLog::new();

        let entry = AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            context_json: r#"{"test": "data"}"#.to_string(),
            hash: String::new(),
        };

        log.append(entry).unwrap();

        let json = log.export_json().unwrap();
        assert!(json.contains("entries"));
        assert!(json.contains("metadata"));

        let csv = log.export_csv();
        assert!(csv.contains("ID,Timestamp,Hash,Context"));
    }

    #[test]
    fn test_statistics() {
        let mut log = TamperEvidentLog::new();

        let entry = AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            context_json: r#"{"test": "data"}"#.to_string(),
            hash: String::new(),
        };

        log.append(entry).unwrap();

        let stats = log.statistics();
        assert_eq!(stats.total_entries, 1);
        assert!(stats.verified);
    }
}
