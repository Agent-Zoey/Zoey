//! Multi-Agent Coordination
//!
//! Enables agents to communicate, collaborate, and help each other

use crate::types::*;
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Agent coordination message
#[derive(Debug, Clone)]
pub struct CoordinationMessage {
    /// Message ID
    pub id: uuid::Uuid,

    /// Source agent ID
    pub from_agent_id: uuid::Uuid,

    /// Target agent ID
    pub to_agent_id: uuid::Uuid,

    /// Message type
    pub message_type: CoordinationMessageType,

    /// Message content
    pub content: serde_json::Value,

    /// Priority (higher = more urgent)
    pub priority: i32,

    /// Timestamp
    pub timestamp: i64,

    /// Optional response required
    pub requires_response: bool,
}

/// Types of coordination messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationMessageType {
    /// Request help from another agent
    HelpRequest,

    /// Offer help to another agent
    HelpOffer,

    /// Share information
    InformationShare,

    /// Delegate task
    TaskDelegation,

    /// Query capabilities
    CapabilityQuery,

    /// Status update
    StatusUpdate,

    /// Generic message
    Generic,
}

/// Agent capability description
#[derive(Debug, Clone)]
pub struct AgentCapability {
    /// Agent ID
    pub agent_id: uuid::Uuid,

    /// Capability name
    pub name: String,

    /// Capability description
    pub description: String,

    /// Proficiency level (0.0 - 1.0)
    pub proficiency: f32,

    /// Availability (0.0 - 1.0, where 1.0 = fully available)
    pub availability: f32,
}

/// Multi-agent coordinator
pub struct MultiAgentCoordinator {
    /// Registered agents
    agents: Arc<RwLock<HashMap<uuid::Uuid, AgentInfo>>>,

    /// Message queue
    messages: Arc<RwLock<Vec<CoordinationMessage>>>,

    /// Agent capabilities
    capabilities: Arc<RwLock<HashMap<uuid::Uuid, Vec<AgentCapability>>>>,
}

/// Agent information
#[derive(Debug, Clone)]
pub struct AgentInfo {
    /// Agent ID
    pub id: uuid::Uuid,

    /// Agent name
    pub name: String,

    /// Agent status
    pub status: AgentStatus,

    /// Current load (0.0 - 1.0)
    pub load: f32,

    /// Last heartbeat
    pub last_heartbeat: i64,
}

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// Agent is online and available
    Online,

    /// Agent is busy
    Busy,

    /// Agent is idle
    Idle,

    /// Agent is offline
    Offline,
}

impl MultiAgentCoordinator {
    /// Create a new multi-agent coordinator
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(Vec::new())),
            capabilities: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an agent
    pub fn register_agent(&self, agent_id: uuid::Uuid, name: String) -> Result<()> {
        info!("Registering agent {} ({})", name, agent_id);

        let info = AgentInfo {
            id: agent_id,
            name,
            status: AgentStatus::Online,
            load: 0.0,
            last_heartbeat: chrono::Utc::now().timestamp(),
        };

        self.agents.write().unwrap().insert(agent_id, info);

        Ok(())
    }

    /// Unregister an agent
    pub fn unregister_agent(&self, agent_id: uuid::Uuid) -> Result<()> {
        info!("Unregistering agent {}", agent_id);
        self.agents.write().unwrap().remove(&agent_id);
        Ok(())
    }

    /// Send message to another agent
    pub fn send_message(&self, message: CoordinationMessage) -> Result<()> {
        debug!(
            "Sending coordination message: {} -> {}",
            message.from_agent_id, message.to_agent_id
        );

        self.messages.write().unwrap().push(message);

        Ok(())
    }

    /// Get messages for an agent
    pub fn get_messages(&self, agent_id: uuid::Uuid) -> Vec<CoordinationMessage> {
        let mut messages = self.messages.write().unwrap();

        // Extract messages for this agent
        let agent_messages: Vec<_> = messages
            .iter()
            .filter(|m| m.to_agent_id == agent_id)
            .cloned()
            .collect();

        // Remove from queue
        messages.retain(|m| m.to_agent_id != agent_id);

        agent_messages
    }

    /// Register agent capability
    pub fn register_capability(&self, capability: AgentCapability) -> Result<()> {
        debug!(
            "Registering capability {} for agent {}",
            capability.name, capability.agent_id
        );

        self.capabilities
            .write()
            .unwrap()
            .entry(capability.agent_id)
            .or_insert_with(Vec::new)
            .push(capability);

        Ok(())
    }

    /// Find agents with specific capability
    pub fn find_agents_with_capability(&self, capability_name: &str) -> Vec<(uuid::Uuid, f32)> {
        let capabilities = self.capabilities.read().unwrap();
        let agents = self.agents.read().unwrap();

        let mut matches = Vec::new();

        for (agent_id, caps) in capabilities.iter() {
            if let Some(agent_info) = agents.get(agent_id) {
                // Only consider online agents
                if agent_info.status == AgentStatus::Online
                    || agent_info.status == AgentStatus::Idle
                {
                    for cap in caps {
                        if cap.name == capability_name {
                            let score =
                                cap.proficiency * cap.availability * (1.0 - agent_info.load);
                            matches.push((*agent_id, score));
                        }
                    }
                }
            }
        }

        // Sort by score (highest first)
        matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        matches
    }

    /// Request help from another agent
    pub async fn request_help(
        &self,
        from_agent_id: uuid::Uuid,
        capability_needed: &str,
        request_data: serde_json::Value,
    ) -> Result<Option<uuid::Uuid>> {
        // Find best agent for the task
        let candidates = self.find_agents_with_capability(capability_needed);

        if let Some((best_agent_id, score)) = candidates.first() {
            info!(
                "Found agent {} for capability {} (score: {})",
                best_agent_id, capability_needed, score
            );

            // Send help request
            let message = CoordinationMessage {
                id: uuid::Uuid::new_v4(),
                from_agent_id,
                to_agent_id: *best_agent_id,
                message_type: CoordinationMessageType::HelpRequest,
                content: request_data,
                priority: 5,
                timestamp: chrono::Utc::now().timestamp(),
                requires_response: true,
            };

            self.send_message(message)?;

            Ok(Some(*best_agent_id))
        } else {
            debug!("No agents found with capability {}", capability_needed);
            Ok(None)
        }
    }

    /// Update agent status
    pub fn update_agent_status(
        &self,
        agent_id: uuid::Uuid,
        status: AgentStatus,
        load: f32,
    ) -> Result<()> {
        if let Some(agent) = self.agents.write().unwrap().get_mut(&agent_id) {
            agent.status = status;
            agent.load = load;
            agent.last_heartbeat = chrono::Utc::now().timestamp();
        }
        Ok(())
    }

    /// Get all active agents
    pub fn get_active_agents(&self) -> Vec<AgentInfo> {
        self.agents
            .read()
            .unwrap()
            .values()
            .filter(|a| a.status != AgentStatus::Offline)
            .cloned()
            .collect()
    }

    /// Broadcast message to all agents
    pub fn broadcast(
        &self,
        from_agent_id: uuid::Uuid,
        content: serde_json::Value,
    ) -> Result<usize> {
        let agents = self.get_active_agents();
        let mut sent = 0;

        for agent in agents {
            if agent.id != from_agent_id {
                let message = CoordinationMessage {
                    id: uuid::Uuid::new_v4(),
                    from_agent_id,
                    to_agent_id: agent.id,
                    message_type: CoordinationMessageType::InformationShare,
                    content: content.clone(),
                    priority: 3,
                    timestamp: chrono::Utc::now().timestamp(),
                    requires_response: false,
                };

                self.send_message(message)?;
                sent += 1;
            }
        }

        Ok(sent)
    }
}

impl Default for MultiAgentCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-agent coordination service
pub struct MultiAgentService {
    coordinator: Arc<MultiAgentCoordinator>,
    agent_id: uuid::Uuid,
}

impl MultiAgentService {
    /// Create a new multi-agent service
    pub fn new(coordinator: Arc<MultiAgentCoordinator>, agent_id: uuid::Uuid) -> Self {
        Self {
            coordinator,
            agent_id,
        }
    }

    /// Request help from another agent
    pub async fn request_help(
        &self,
        capability: &str,
        data: serde_json::Value,
    ) -> Result<Option<uuid::Uuid>> {
        self.coordinator
            .request_help(self.agent_id, capability, data)
            .await
    }

    /// Offer help for a capability
    pub fn offer_capability(
        &self,
        name: String,
        description: String,
        proficiency: f32,
    ) -> Result<()> {
        let capability = AgentCapability {
            agent_id: self.agent_id,
            name,
            description,
            proficiency,
            availability: 1.0,
        };

        self.coordinator.register_capability(capability)
    }

    /// Get pending messages
    pub fn get_messages(&self) -> Vec<CoordinationMessage> {
        self.coordinator.get_messages(self.agent_id)
    }

    /// Update status
    pub fn update_status(&self, status: AgentStatus, load: f32) -> Result<()> {
        self.coordinator
            .update_agent_status(self.agent_id, status, load)
    }

    /// Find agents offering a capability and return (agent_id, score)
    pub fn find_agents(&self, capability: &str) -> Vec<(uuid::Uuid, f32)> {
        self.coordinator.find_agents_with_capability(capability)
    }
}

#[async_trait]
impl Service for MultiAgentService {
    fn service_type(&self) -> &str {
        "multi-agent-coordination"
    }

    async fn initialize(&mut self, _runtime: Arc<dyn std::any::Any + Send + Sync>) -> Result<()> {
        info!("Multi-agent coordination service initialized");
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        info!("Multi-agent coordination service started");
        Ok(())
    }

    fn query_agents(&self, capability: &str) -> Option<Vec<(uuid::Uuid, f32)>> {
        Some(self.coordinator.find_agents_with_capability(capability))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_creation() {
        let coordinator = MultiAgentCoordinator::new();
        assert_eq!(coordinator.get_active_agents().len(), 0);
    }

    #[test]
    fn test_agent_registration() {
        let coordinator = MultiAgentCoordinator::new();

        let agent_id = uuid::Uuid::new_v4();
        coordinator
            .register_agent(agent_id, "TestAgent".to_string())
            .unwrap();

        assert_eq!(coordinator.get_active_agents().len(), 1);
    }

    #[test]
    fn test_capability_registration() {
        let coordinator = MultiAgentCoordinator::new();

        let agent_id = uuid::Uuid::new_v4();
        coordinator
            .register_agent(agent_id, "Agent1".to_string())
            .unwrap();

        let capability = AgentCapability {
            agent_id,
            name: "code_generation".to_string(),
            description: "Can generate code".to_string(),
            proficiency: 0.9,
            availability: 1.0,
        };

        coordinator.register_capability(capability).unwrap();

        let matches = coordinator.find_agents_with_capability("code_generation");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, agent_id);
    }

    #[tokio::test]
    async fn test_help_request() {
        let coordinator = MultiAgentCoordinator::new();

        let agent1 = uuid::Uuid::new_v4();
        let agent2 = uuid::Uuid::new_v4();

        coordinator
            .register_agent(agent1, "Agent1".to_string())
            .unwrap();
        coordinator
            .register_agent(agent2, "Agent2".to_string())
            .unwrap();

        // Agent2 has a capability
        let capability = AgentCapability {
            agent_id: agent2,
            name: "translation".to_string(),
            description: "Can translate text".to_string(),
            proficiency: 0.95,
            availability: 1.0,
        };
        coordinator.register_capability(capability).unwrap();

        // Agent1 requests help
        let result = coordinator
            .request_help(
                agent1,
                "translation",
                serde_json::json!({"text": "Hello", "to_lang": "Spanish"}),
            )
            .await
            .unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), agent2);

        // Agent2 should have received the message
        let messages = coordinator.get_messages(agent2);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].message_type,
            CoordinationMessageType::HelpRequest
        );
    }

    #[test]
    fn test_broadcast() {
        let coordinator = MultiAgentCoordinator::new();

        let agent1 = uuid::Uuid::new_v4();
        let agent2 = uuid::Uuid::new_v4();
        let agent3 = uuid::Uuid::new_v4();

        coordinator
            .register_agent(agent1, "Agent1".to_string())
            .unwrap();
        coordinator
            .register_agent(agent2, "Agent2".to_string())
            .unwrap();
        coordinator
            .register_agent(agent3, "Agent3".to_string())
            .unwrap();

        let sent = coordinator
            .broadcast(agent1, serde_json::json!({"info": "test"}))
            .unwrap();

        assert_eq!(sent, 2); // Sent to agent2 and agent3 (not agent1)

        assert_eq!(coordinator.get_messages(agent2).len(), 1);
        assert_eq!(coordinator.get_messages(agent3).len(), 1);
        assert_eq!(coordinator.get_messages(agent1).len(), 0);
    }
}
