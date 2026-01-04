//! UUID utility functions

use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Create a deterministic UUID from an agent ID and input string
pub fn create_unique_uuid(agent_id: Uuid, input: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(agent_id.as_bytes());
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();

    // Take first 16 bytes of hash to create UUID
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[0..16]);

    // Set version to 4 (random) and variant to RFC4122
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    Uuid::from_bytes(bytes)
}

/// Create a deterministic UUID from a string
pub fn string_to_uuid(input: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();

    // Take first 16 bytes of hash to create UUID
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[0..16]);

    // Set version to 4 (random) and variant to RFC4122
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_to_uuid_deterministic() {
        let uuid1 = string_to_uuid("test_string");
        let uuid2 = string_to_uuid("test_string");
        assert_eq!(uuid1, uuid2);
    }

    #[test]
    fn test_string_to_uuid_different() {
        let uuid1 = string_to_uuid("test1");
        let uuid2 = string_to_uuid("test2");
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_create_unique_uuid_deterministic() {
        let agent_id = Uuid::new_v4();
        let uuid1 = create_unique_uuid(agent_id, "test");
        let uuid2 = create_unique_uuid(agent_id, "test");
        assert_eq!(uuid1, uuid2);
    }

    #[test]
    fn test_create_unique_uuid_agent_specific() {
        let agent_id1 = Uuid::new_v4();
        let agent_id2 = Uuid::new_v4();
        let uuid1 = create_unique_uuid(agent_id1, "test");
        let uuid2 = create_unique_uuid(agent_id2, "test");
        assert_ne!(uuid1, uuid2);
    }
}
