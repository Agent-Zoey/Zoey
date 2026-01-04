//! Role management utilities for worlds/servers

use crate::types::{World, UUID};
use serde::{Deserialize, Serialize};

/// Role enum representing different permission levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    /// No role/permissions
    None,
    /// Regular member
    Member,
    /// Moderator with elevated permissions
    Moderator,
    /// Administrator with full permissions
    Admin,
    /// Owner with complete control
    Owner,
}

impl Default for Role {
    fn default() -> Self {
        Role::None
    }
}

impl Role {
    /// Check if this role has at least the given permission level
    pub fn has_permission(&self, required: Role) -> bool {
        let self_level = self.level();
        let required_level = required.level();
        self_level >= required_level
    }

    /// Get numeric level for role comparison
    fn level(&self) -> u8 {
        match self {
            Role::None => 0,
            Role::Member => 1,
            Role::Moderator => 2,
            Role::Admin => 3,
            Role::Owner => 4,
        }
    }
}

/// Gets a user's role from world metadata
///
/// # Arguments
/// * `_agent_id` - The agent ID (for future use)
/// * `entity_id` - The entity ID
/// * `world` - The world object
///
/// # Returns
/// The role of the entity in the world
pub fn get_user_world_role(_agent_id: UUID, entity_id: UUID, world: &World) -> Role {
    // Check if world has roles metadata
    if let Some(roles_value) = world.metadata.get("roles") {
        if let Some(roles_map) = roles_value.as_object() {
            // Check entity ID
            if let Some(role_value) = roles_map.get(&entity_id.to_string()) {
                if let Some(role_str) = role_value.as_str() {
                    return parse_role(role_str);
                }
            }
        }
    }

    Role::None
}

/// Parse role from string
fn parse_role(role_str: &str) -> Role {
    match role_str.to_uppercase().as_str() {
        "OWNER" => Role::Owner,
        "ADMIN" => Role::Admin,
        "MODERATOR" | "MOD" => Role::Moderator,
        "MEMBER" => Role::Member,
        _ => Role::None,
    }
}

/// Find worlds where the given entity is the owner
///
/// # Arguments
/// * `_agent_id` - The agent ID (for future use)
/// * `entity_id` - The entity ID
/// * `worlds` - List of all worlds to search
///
/// # Returns
/// A vector of worlds where the entity is the owner
pub fn find_worlds_for_owner(_agent_id: UUID, entity_id: UUID, worlds: &[World]) -> Vec<World> {
    worlds
        .iter()
        .filter(|world| {
            // Check if the entity is the owner in world metadata
            if let Some(ownership_value) = world.metadata.get("ownership") {
                if let Some(ownership_obj) = ownership_value.as_object() {
                    if let Some(owner_id_value) = ownership_obj.get("ownerId") {
                        if let Some(owner_id_str) = owner_id_value.as_str() {
                            return owner_id_str == entity_id.to_string();
                        }
                    }
                }
            }
            false
        })
        .cloned()
        .collect()
}

/// Check if an entity is an admin or owner in a world
///
/// # Arguments
/// * `agent_id` - The agent ID
/// * `entity_id` - The entity ID
/// * `world` - The world to check
///
/// # Returns
/// True if the entity is an admin or owner
pub fn is_admin_or_owner(agent_id: UUID, entity_id: UUID, world: &World) -> bool {
    let role = get_user_world_role(agent_id, entity_id, world);
    role == Role::Admin || role == Role::Owner
}

/// Check if an entity is a moderator or higher in a world
///
/// # Arguments
/// * `agent_id` - The agent ID
/// * `entity_id` - The entity ID
/// * `world` - The world to check
///
/// # Returns
/// True if the entity is a moderator, admin, or owner
pub fn is_moderator_or_higher(agent_id: UUID, entity_id: UUID, world: &World) -> bool {
    let role = get_user_world_role(agent_id, entity_id, world);
    role.has_permission(Role::Moderator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Metadata;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn test_role_levels() {
        assert_eq!(Role::None.level(), 0);
        assert_eq!(Role::Member.level(), 1);
        assert_eq!(Role::Moderator.level(), 2);
        assert_eq!(Role::Admin.level(), 3);
        assert_eq!(Role::Owner.level(), 4);
    }

    #[test]
    fn test_role_permissions() {
        assert!(Role::Owner.has_permission(Role::Admin));
        assert!(Role::Admin.has_permission(Role::Moderator));
        assert!(Role::Moderator.has_permission(Role::Member));
        assert!(!Role::Member.has_permission(Role::Moderator));
    }

    #[test]
    fn test_parse_role() {
        assert_eq!(parse_role("OWNER"), Role::Owner);
        assert_eq!(parse_role("ADMIN"), Role::Admin);
        assert_eq!(parse_role("MODERATOR"), Role::Moderator);
        assert_eq!(parse_role("MEMBER"), Role::Member);
        assert_eq!(parse_role("unknown"), Role::None);
    }

    #[test]
    fn test_get_user_world_role() {
        let agent_id = Uuid::new_v4();
        let entity_id = Uuid::new_v4();

        let mut metadata = Metadata::new();
        metadata.insert(
            "roles".to_string(),
            json!({
                entity_id.to_string(): "ADMIN"
            }),
        );

        let world = World {
            id: Uuid::new_v4(),
            name: "Test World".to_string(),
            agent_id,
            server_id: None,
            metadata,
            created_at: Some(12345),
        };

        let role = get_user_world_role(agent_id, entity_id, &world);
        assert_eq!(role, Role::Admin);
    }

    #[test]
    fn test_find_worlds_for_owner() {
        let agent_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let mut metadata = Metadata::new();
        metadata.insert(
            "ownership".to_string(),
            json!({
                "ownerId": owner_id.to_string()
            }),
        );

        let world = World {
            id: Uuid::new_v4(),
            name: "Test World".to_string(),
            agent_id,
            server_id: None,
            metadata,
            created_at: Some(12345),
        };

        let worlds = find_worlds_for_owner(agent_id, owner_id, &[world.clone()]);
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].id, world.id);
    }
}
