//! Distributed Runtime Support
//!
//! Enables agents to run across multiple nodes/processes

use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node ID
    pub id: uuid::Uuid,

    /// Node name/hostname
    pub name: String,

    /// Node address (IP:port)
    pub address: String,

    /// Node status
    pub status: NodeStatus,

    /// Agents running on this node
    pub agents: Vec<uuid::Uuid>,

    /// CPU usage (0.0 - 1.0)
    pub cpu_usage: f32,

    /// Memory usage (0.0 - 1.0)
    pub memory_usage: f32,

    /// Last heartbeat timestamp
    pub last_heartbeat: i64,
}

/// Node status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is healthy and operational
    Healthy,

    /// Node is degraded but functional
    Degraded,

    /// Node is unhealthy
    Unhealthy,

    /// Node is offline
    Offline,
}

/// Distributed message for cross-node communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedMessage {
    /// Message ID
    pub id: uuid::Uuid,

    /// Source node ID
    pub from_node: uuid::Uuid,

    /// Target node ID
    pub to_node: uuid::Uuid,

    /// Source agent ID
    pub from_agent: uuid::Uuid,

    /// Target agent ID
    pub to_agent: uuid::Uuid,

    /// Message payload
    pub payload: serde_json::Value,

    /// Message type
    pub message_type: String,

    /// Timestamp
    pub timestamp: i64,
}

/// Distributed runtime coordinator
pub struct DistributedRuntime {
    /// This node's ID
    node_id: uuid::Uuid,

    /// Registered nodes
    nodes: Arc<RwLock<HashMap<uuid::Uuid, NodeInfo>>>,

    /// Message sender
    message_tx: mpsc::UnboundedSender<DistributedMessage>,

    /// Message receiver for processing incoming messages
    message_rx: Arc<RwLock<mpsc::UnboundedReceiver<DistributedMessage>>>,

    /// Agent-to-node mapping
    agent_locations: Arc<RwLock<HashMap<uuid::Uuid, uuid::Uuid>>>,

    /// Count of pending messages in the queue
    pending_count: Arc<AtomicUsize>,

    /// Total messages sent
    messages_sent: Arc<AtomicUsize>,

    /// Total messages received
    messages_received: Arc<AtomicUsize>,
}

impl DistributedRuntime {
    /// Create a new distributed runtime
    pub fn new(node_id: uuid::Uuid) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        Self {
            node_id,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            message_tx: tx,
            message_rx: Arc::new(RwLock::new(rx)),
            agent_locations: Arc::new(RwLock::new(HashMap::new())),
            pending_count: Arc::new(AtomicUsize::new(0)),
            messages_sent: Arc::new(AtomicUsize::new(0)),
            messages_received: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Register a node in the cluster
    pub fn register_node(&self, node: NodeInfo) -> Result<()> {
        info!("Registering node {} at {}", node.name, node.address);
        debug!("Node {} has {} agents", node.id, node.agents.len());

        // Update agent locations
        for agent_id in &node.agents {
            debug!("Mapping agent {} to node {}", agent_id, node.id);
            self.agent_locations
                .write()
                .unwrap()
                .insert(*agent_id, node.id);
        }

        self.nodes.write().unwrap().insert(node.id, node);
        debug!(
            "Total nodes in cluster: {}",
            self.nodes.read().unwrap().len()
        );

        Ok(())
    }

    /// Unregister a node
    pub fn unregister_node(&self, node_id: uuid::Uuid) -> Result<()> {
        info!("Unregistering node {}", node_id);

        if let Some(node) = self.nodes.write().unwrap().remove(&node_id) {
            // Remove agent locations
            for agent_id in &node.agents {
                self.agent_locations.write().unwrap().remove(agent_id);
            }
        }

        Ok(())
    }

    /// Send message to agent on any node
    pub async fn send_to_agent(
        &self,
        from_agent: uuid::Uuid,
        to_agent: uuid::Uuid,
        payload: serde_json::Value,
        message_type: String,
    ) -> Result<()> {
        debug!(
            "Sending {} message from agent {} to agent {}",
            message_type, from_agent, to_agent
        );

        // Find target node
        let to_node = self
            .agent_locations
            .read()
            .unwrap()
            .get(&to_agent)
            .copied()
            .ok_or_else(|| {
                ZoeyError::not_found(format!("Agent {} not found in cluster", to_agent))
            })?;

        debug!("Target agent {} is on node {}", to_agent, to_node);

        let message = DistributedMessage {
            id: uuid::Uuid::new_v4(),
            from_node: self.node_id,
            to_node,
            from_agent,
            to_agent,
            payload,
            message_type: message_type.clone(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        // Send via message queue
        self.message_tx
            .send(message)
            .map_err(|e| ZoeyError::other(format!("Failed to send message: {}", e)))?;

        // Update counters
        self.pending_count.fetch_add(1, Ordering::SeqCst);
        self.messages_sent.fetch_add(1, Ordering::SeqCst);

        debug!(
            "Message queued successfully (pending: {})",
            self.pending_count.load(Ordering::SeqCst)
        );
        Ok(())
    }

    /// Get node for agent
    pub fn get_agent_node(&self, agent_id: uuid::Uuid) -> Option<uuid::Uuid> {
        self.agent_locations.read().unwrap().get(&agent_id).copied()
    }

    /// Get all nodes
    pub fn get_nodes(&self) -> Vec<NodeInfo> {
        self.nodes.read().unwrap().values().cloned().collect()
    }

    /// Get healthy nodes
    pub fn get_healthy_nodes(&self) -> Vec<NodeInfo> {
        self.nodes
            .read()
            .unwrap()
            .values()
            .filter(|n| n.status == NodeStatus::Healthy)
            .cloned()
            .collect()
    }

    /// Find best node for new agent (load balancing)
    pub fn find_best_node(&self) -> Option<uuid::Uuid> {
        let nodes = self.get_healthy_nodes();

        if nodes.is_empty() {
            warn!("No healthy nodes available for load balancing");
            return None;
        }

        debug!("Finding best node among {} healthy nodes", nodes.len());

        // Find node with lowest combined load
        let best = nodes
            .iter()
            .min_by(|a, b| {
                let load_a = a.cpu_usage + a.memory_usage;
                let load_b = b.cpu_usage + b.memory_usage;
                load_a
                    .partial_cmp(&load_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|n| {
                let load = n.cpu_usage + n.memory_usage;
                debug!("Selected node {} with load {:.2}", n.name, load);
                n.id
            });

        best
    }

    /// Heartbeat to update node status
    pub fn heartbeat(&self, node_id: uuid::Uuid, cpu_usage: f32, memory_usage: f32) -> Result<()> {
        if let Some(node) = self.nodes.write().unwrap().get_mut(&node_id) {
            let old_status = node.status;
            node.cpu_usage = cpu_usage;
            node.memory_usage = memory_usage;
            node.last_heartbeat = chrono::Utc::now().timestamp();

            // Update status based on health
            node.status = if cpu_usage > 0.9 || memory_usage > 0.9 {
                NodeStatus::Degraded
            } else if cpu_usage > 0.95 || memory_usage > 0.95 {
                NodeStatus::Unhealthy
            } else {
                NodeStatus::Healthy
            };

            // Log status changes
            if old_status != node.status {
                info!(
                    "Node {} status changed: {:?} -> {:?}",
                    node.name, old_status, node.status
                );
            }
            debug!(
                "Node {} heartbeat: CPU {:.1}%, Memory {:.1}%",
                node.name,
                cpu_usage * 100.0,
                memory_usage * 100.0
            );
        } else {
            warn!("Received heartbeat from unknown node {}", node_id);
        }

        Ok(())
    }

    /// Check for dead nodes (no heartbeat)
    pub fn check_node_health(&self, timeout_seconds: i64) -> Vec<uuid::Uuid> {
        let now = chrono::Utc::now().timestamp();
        let mut dead_nodes = Vec::new();

        for (node_id, node) in self.nodes.read().unwrap().iter() {
            if now - node.last_heartbeat > timeout_seconds {
                warn!(
                    "Node {} hasn't sent heartbeat for {} seconds",
                    node.name,
                    now - node.last_heartbeat
                );
                dead_nodes.push(*node_id);
            }
        }

        dead_nodes
    }

    /// Try to receive a message from the queue (non-blocking)
    pub fn try_recv_message(&self) -> Option<DistributedMessage> {
        let mut rx = self.message_rx.write().unwrap();
        match rx.try_recv() {
            Ok(msg) => {
                // Decrement pending count and increment received count
                self.pending_count.fetch_sub(1, Ordering::SeqCst);
                self.messages_received.fetch_add(1, Ordering::SeqCst);
                Some(msg)
            }
            Err(_) => None,
        }
    }

    /// Receive messages with a handler (blocking until handler returns)
    pub async fn receive_messages<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(
                DistributedMessage,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
            + Send,
    {
        loop {
            let message = {
                let mut rx = self.message_rx.write().unwrap();
                match rx.try_recv() {
                    Ok(msg) => Some(msg),
                    Err(mpsc::error::TryRecvError::Empty) => None,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        warn!("Message channel disconnected");
                        return Err(ZoeyError::other("Message channel disconnected"));
                    }
                }
            };

            if let Some(msg) = message {
                debug!("Received message {} from node {}", msg.id, msg.from_node);
                handler(msg).await?;
            } else {
                // No messages available, yield
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }

    /// Process pending messages with a batch handler
    pub async fn process_pending_messages<F>(&self, handler: F) -> Result<usize>
    where
        F: Fn(&DistributedMessage) -> Result<()>,
    {
        let mut processed = 0;

        loop {
            let message = self.try_recv_message();

            match message {
                Some(msg) => {
                    debug!("Processing message {} type={}", msg.id, msg.message_type);

                    // Validate message is for this node
                    if msg.to_node != self.node_id {
                        warn!(
                            "Received message for wrong node: expected {}, got {}",
                            self.node_id, msg.to_node
                        );
                        continue;
                    }

                    match handler(&msg) {
                        Ok(_) => {
                            processed += 1;
                            debug!("Successfully processed message {}", msg.id);
                        }
                        Err(e) => {
                            warn!("Failed to process message {}: {}", msg.id, e);
                        }
                    }
                }
                None => {
                    // No more messages in queue
                    break;
                }
            }
        }

        if processed > 0 {
            info!("Processed {} distributed message(s)", processed);
        }

        Ok(processed)
    }

    /// Get the number of pending messages in the queue
    pub fn pending_message_count(&self) -> usize {
        self.pending_count.load(Ordering::SeqCst)
    }

    /// Get the total number of messages sent through this node
    pub fn total_messages_sent(&self) -> usize {
        self.messages_sent.load(Ordering::SeqCst)
    }

    /// Get the total number of messages received by this node
    pub fn total_messages_received(&self) -> usize {
        self.messages_received.load(Ordering::SeqCst)
    }

    /// Get message processing statistics
    pub fn get_message_stats(&self) -> MessageStats {
        MessageStats {
            pending: self.pending_count.load(Ordering::SeqCst),
            sent: self.messages_sent.load(Ordering::SeqCst),
            received: self.messages_received.load(Ordering::SeqCst),
        }
    }

    /// Reset message statistics
    pub fn reset_message_stats(&self) {
        self.messages_sent.store(0, Ordering::SeqCst);
        self.messages_received.store(0, Ordering::SeqCst);
        info!("Message statistics reset for node {}", self.node_id);
    }
}

/// Message processing statistics
#[derive(Debug, Clone, Copy)]
pub struct MessageStats {
    /// Number of pending messages
    pub pending: usize,
    /// Total messages sent
    pub sent: usize,
    /// Total messages received
    pub received: usize,
}

/// Cluster configuration
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// Heartbeat interval
    pub heartbeat_interval: Duration,

    /// Node timeout before considering dead
    pub node_timeout: Duration,

    /// Enable automatic rebalancing
    pub auto_rebalance: bool,

    /// Replication factor
    pub replication_factor: usize,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(5),
            node_timeout: Duration::from_secs(30),
            auto_rebalance: true,
            replication_factor: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distributed_runtime() {
        let runtime = DistributedRuntime::new(uuid::Uuid::new_v4());
        assert_eq!(runtime.get_nodes().len(), 0);
    }

    #[test]
    fn test_node_registration() {
        let runtime = DistributedRuntime::new(uuid::Uuid::new_v4());

        let node = NodeInfo {
            id: uuid::Uuid::new_v4(),
            name: "node1".to_string(),
            address: "127.0.0.1:8080".to_string(),
            status: NodeStatus::Healthy,
            agents: vec![],
            cpu_usage: 0.5,
            memory_usage: 0.6,
            last_heartbeat: chrono::Utc::now().timestamp(),
        };

        runtime.register_node(node.clone()).unwrap();

        assert_eq!(runtime.get_nodes().len(), 1);
        assert_eq!(runtime.get_healthy_nodes().len(), 1);
    }

    #[test]
    fn test_load_balancing() {
        let runtime = DistributedRuntime::new(uuid::Uuid::new_v4());

        let node1 = NodeInfo {
            id: uuid::Uuid::new_v4(),
            name: "node1".to_string(),
            address: "127.0.0.1:8080".to_string(),
            status: NodeStatus::Healthy,
            agents: vec![],
            cpu_usage: 0.8, // High load
            memory_usage: 0.7,
            last_heartbeat: chrono::Utc::now().timestamp(),
        };

        let node2 = NodeInfo {
            id: uuid::Uuid::new_v4(),
            name: "node2".to_string(),
            address: "127.0.0.1:8081".to_string(),
            status: NodeStatus::Healthy,
            agents: vec![],
            cpu_usage: 0.3, // Low load
            memory_usage: 0.4,
            last_heartbeat: chrono::Utc::now().timestamp(),
        };

        runtime.register_node(node1).unwrap();
        runtime.register_node(node2.clone()).unwrap();

        // Should select node2 (lower load)
        let best = runtime.find_best_node().unwrap();
        assert_eq!(best, node2.id);
    }

    #[tokio::test]
    async fn test_cross_node_messaging() {
        let runtime = DistributedRuntime::new(uuid::Uuid::new_v4());

        let agent1 = uuid::Uuid::new_v4();
        let agent2 = uuid::Uuid::new_v4();

        let node1 = NodeInfo {
            id: uuid::Uuid::new_v4(),
            name: "node1".to_string(),
            address: "127.0.0.1:8080".to_string(),
            status: NodeStatus::Healthy,
            agents: vec![agent1],
            cpu_usage: 0.5,
            memory_usage: 0.5,
            last_heartbeat: chrono::Utc::now().timestamp(),
        };

        let node2 = NodeInfo {
            id: uuid::Uuid::new_v4(),
            name: "node2".to_string(),
            address: "127.0.0.1:8081".to_string(),
            status: NodeStatus::Healthy,
            agents: vec![agent2],
            cpu_usage: 0.5,
            memory_usage: 0.5,
            last_heartbeat: chrono::Utc::now().timestamp(),
        };

        runtime.register_node(node1).unwrap();
        runtime.register_node(node2).unwrap();

        // Send message from agent1 to agent2 (cross-node)
        let result = runtime
            .send_to_agent(
                agent1,
                agent2,
                serde_json::json!({"message": "Hello from another node!"}),
                "greeting".to_string(),
            )
            .await;

        assert!(result.is_ok());
    }
}
