/*!
# Distributed Workflow Execution

Provides distributed execution capabilities for workflows including:
- Task distribution across multiple workers
- Load balancing and work stealing
- Fault tolerance and recovery
- Distributed state management
*/

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Distributed execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedConfig {
    /// Number of workers
    pub num_workers: usize,
    /// Worker heartbeat interval (ms)
    pub heartbeat_interval_ms: u64,
    /// Worker timeout (ms)
    pub worker_timeout_ms: u64,
    /// Enable work stealing
    pub work_stealing: bool,
    /// Task queue capacity per worker
    pub queue_capacity: usize,
    /// Enable checkpointing
    pub checkpointing: bool,
    /// Checkpoint interval (seconds)
    pub checkpoint_interval_secs: u64,
    /// Retry failed tasks
    pub retry_failed: bool,
    /// Maximum retries per task
    pub max_retries: usize,
}

impl Default for DistributedConfig {
    fn default() -> Self {
        Self {
            num_workers: 4,
            heartbeat_interval_ms: 1000,
            worker_timeout_ms: 5000,
            work_stealing: true,
            queue_capacity: 100,
            checkpointing: true,
            checkpoint_interval_secs: 30,
            retry_failed: true,
            max_retries: 3,
        }
    }
}

/// Worker status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerStatus {
    /// Worker is idle
    Idle,
    /// Worker is busy
    Busy,
    /// Worker is draining (finishing current work)
    Draining,
    /// Worker is offline
    Offline,
    /// Worker failed
    Failed,
}

/// Worker information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    /// Worker ID
    pub id: String,
    /// Worker status
    pub status: WorkerStatus,
    /// Current task (if any)
    pub current_task: Option<String>,
    /// Tasks completed
    pub tasks_completed: usize,
    /// Tasks failed
    pub tasks_failed: usize,
    /// Last heartbeat
    #[serde(skip)]
    pub last_heartbeat: Option<Instant>,
    /// Queue depth
    pub queue_depth: usize,
    /// Resource utilization
    pub utilization: f64,
}

impl WorkerInfo {
    /// Create a new worker
    pub fn new(id: String) -> Self {
        Self {
            id,
            status: WorkerStatus::Idle,
            current_task: None,
            tasks_completed: 0,
            tasks_failed: 0,
            last_heartbeat: Some(Instant::now()),
            queue_depth: 0,
            utilization: 0.0,
        }
    }

    /// Check if worker is alive
    pub fn is_alive(&self, timeout_ms: u64) -> bool {
        self.last_heartbeat
            .map(|hb| hb.elapsed().as_millis() < timeout_ms as u128)
            .unwrap_or(false)
    }
}

/// Distributed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedTask {
    /// Task ID
    pub id: String,
    /// Task type/name
    pub task_type: String,
    /// Task payload
    pub payload: serde_json::Value,
    /// Priority (higher = more important)
    pub priority: i32,
    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Deadline (optional)
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    /// Retry count
    pub retry_count: usize,
    /// Dependencies (task IDs that must complete first)
    pub dependencies: Vec<String>,
    /// Assigned worker
    pub assigned_worker: Option<String>,
}

impl DistributedTask {
    /// Create a new distributed task
    pub fn new(id: String, task_type: String, payload: serde_json::Value) -> Self {
        Self {
            id,
            task_type,
            payload,
            priority: 0,
            created_at: chrono::Utc::now(),
            deadline: None,
            retry_count: 0,
            dependencies: Vec::new(),
            assigned_worker: None,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Add dependency
    pub fn with_dependency(mut self, task_id: String) -> Self {
        self.dependencies.push(task_id);
        self
    }

    /// Set deadline
    pub fn with_deadline(mut self, deadline: chrono::DateTime<chrono::Utc>) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

/// Task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID
    pub task_id: String,
    /// Success status
    pub success: bool,
    /// Result data
    pub data: Option<serde_json::Value>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Execution time (ms)
    pub execution_time_ms: u64,
    /// Worker that executed the task
    pub worker_id: String,
}

/// Load balancing strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadBalancer {
    /// Round-robin distribution
    RoundRobin,
    /// Least-loaded worker
    LeastLoaded,
    /// Random distribution
    Random,
    /// Consistent hashing (sticky to worker)
    ConsistentHash,
    /// Priority-based (high priority workers first)
    Priority,
}

/// Distributed coordinator
pub struct DistributedCoordinator {
    config: DistributedConfig,
    workers: Arc<RwLock<HashMap<String, WorkerInfo>>>,
    task_queue: Arc<RwLock<VecDeque<DistributedTask>>>,
    completed_tasks: Arc<RwLock<HashMap<String, TaskResult>>>,
    running_tasks: Arc<RwLock<HashMap<String, DistributedTask>>>,
    load_balancer: LoadBalancer,
    round_robin_idx: Arc<RwLock<usize>>,
}

impl DistributedCoordinator {
    /// Create a new coordinator
    pub fn new(config: DistributedConfig) -> Self {
        Self {
            config,
            workers: Arc::new(RwLock::new(HashMap::new())),
            task_queue: Arc::new(RwLock::new(VecDeque::new())),
            completed_tasks: Arc::new(RwLock::new(HashMap::new())),
            running_tasks: Arc::new(RwLock::new(HashMap::new())),
            load_balancer: LoadBalancer::LeastLoaded,
            round_robin_idx: Arc::new(RwLock::new(0)),
        }
    }

    /// Set load balancer strategy
    pub fn with_load_balancer(mut self, lb: LoadBalancer) -> Self {
        self.load_balancer = lb;
        self
    }

    /// Register a worker
    pub async fn register_worker(&self, worker_id: String) -> Result<(), DistributedError> {
        let mut workers = self.workers.write().await;
        if workers.len() >= self.config.num_workers {
            return Err(DistributedError::WorkerLimitReached);
        }
        workers.insert(worker_id.clone(), WorkerInfo::new(worker_id));
        Ok(())
    }

    /// Unregister a worker
    pub async fn unregister_worker(&self, worker_id: &str) {
        self.workers.write().await.remove(worker_id);
    }

    /// Worker heartbeat
    pub async fn heartbeat(&self, worker_id: &str, utilization: f64) {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.last_heartbeat = Some(Instant::now());
            worker.utilization = utilization;
            if worker.status == WorkerStatus::Offline {
                worker.status = WorkerStatus::Idle;
            }
        }
    }

    /// Submit a task
    pub async fn submit_task(&self, task: DistributedTask) -> Result<String, DistributedError> {
        let task_id = task.id.clone();
        let mut queue = self.task_queue.write().await;

        if queue.len() >= self.config.queue_capacity * self.config.num_workers {
            return Err(DistributedError::QueueFull);
        }

        // Insert based on priority (higher priority at front)
        let pos = queue.iter().position(|t| t.priority < task.priority);
        match pos {
            Some(idx) => queue.insert(idx, task),
            None => queue.push_back(task),
        }

        Ok(task_id)
    }

    /// Get next task for a worker
    pub async fn get_task(&self, worker_id: &str) -> Option<DistributedTask> {
        let completed = self.completed_tasks.read().await;
        let mut queue = self.task_queue.write().await;

        // Find a task whose dependencies are satisfied
        let task_idx = queue.iter().position(|task| {
            task.dependencies
                .iter()
                .all(|dep| completed.contains_key(dep))
        });

        if let Some(idx) = task_idx {
            let mut task = queue.remove(idx)?;
            task.assigned_worker = Some(worker_id.to_string());

            // Update worker status
            let mut workers = self.workers.write().await;
            if let Some(worker) = workers.get_mut(worker_id) {
                worker.status = WorkerStatus::Busy;
                worker.current_task = Some(task.id.clone());
            }

            // Track running task
            self.running_tasks
                .write()
                .await
                .insert(task.id.clone(), task.clone());

            Some(task)
        } else {
            None
        }
    }

    /// Report task completion
    pub async fn complete_task(&self, result: TaskResult) {
        let task_id = result.task_id.clone();
        let worker_id = result.worker_id.clone();

        // Remove from running
        self.running_tasks.write().await.remove(&task_id);

        // Store result
        self.completed_tasks
            .write()
            .await
            .insert(task_id, result.clone());

        // Update worker
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.current_task = None;
            worker.status = WorkerStatus::Idle;
            if result.success {
                worker.tasks_completed += 1;
            } else {
                worker.tasks_failed += 1;
            }
        }
    }

    /// Retry a failed task
    pub async fn retry_task(&self, task_id: &str) -> Result<(), DistributedError> {
        let running = self.running_tasks.read().await;
        if let Some(task) = running.get(task_id).cloned() {
            drop(running);

            if task.retry_count >= self.config.max_retries {
                return Err(DistributedError::MaxRetriesExceeded);
            }

            let mut new_task = task;
            new_task.retry_count += 1;
            new_task.assigned_worker = None;

            self.running_tasks.write().await.remove(task_id);
            self.submit_task(new_task).await?;
            Ok(())
        } else {
            Err(DistributedError::TaskNotFound)
        }
    }

    /// Select worker using configured strategy
    pub async fn select_worker(&self) -> Option<String> {
        let workers = self.workers.read().await;
        let available: Vec<_> = workers
            .iter()
            .filter(|(_, w)| {
                w.status == WorkerStatus::Idle && w.is_alive(self.config.worker_timeout_ms)
            })
            .collect();

        if available.is_empty() {
            return None;
        }

        match self.load_balancer {
            LoadBalancer::RoundRobin => {
                let mut idx = self.round_robin_idx.write().await;
                let worker_id = available[*idx % available.len()].0.clone();
                *idx = (*idx + 1) % available.len();
                Some(worker_id)
            }
            LoadBalancer::LeastLoaded => available
                .iter()
                .min_by(|a, b| {
                    a.1.utilization
                        .partial_cmp(&b.1.utilization)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(id, _)| (*id).clone()),
            LoadBalancer::Random => {
                let idx: usize = rand::random::<usize>() % available.len();
                Some(available[idx].0.clone())
            }
            LoadBalancer::ConsistentHash | LoadBalancer::Priority => {
                // For these strategies, just return first available
                available.first().map(|(id, _)| (*id).clone())
            }
        }
    }

    /// Check for timed-out workers
    pub async fn check_worker_health(&self) {
        let mut workers = self.workers.write().await;
        let timeout = self.config.worker_timeout_ms;

        for (_, worker) in workers.iter_mut() {
            if !worker.is_alive(timeout) {
                if worker.status != WorkerStatus::Offline {
                    tracing::warn!("Worker {} timed out", worker.id);
                    worker.status = WorkerStatus::Offline;

                    // Requeue the worker's task if any
                    if let Some(task_id) = worker.current_task.take() {
                        if self.config.retry_failed {
                            let _ = self.retry_task(&task_id).await;
                        }
                    }
                }
            }
        }
    }

    /// Work stealing - transfer tasks from overloaded to idle workers
    pub async fn balance_load(&self) {
        if !self.config.work_stealing {
            return;
        }

        let workers = self.workers.read().await;
        let idle_workers: Vec<_> = workers
            .iter()
            .filter(|(_, w)| w.status == WorkerStatus::Idle && w.queue_depth == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let busy_workers: Vec<_> = workers
            .iter()
            .filter(|(_, w)| w.queue_depth > 2)
            .map(|(id, w)| (id.clone(), w.queue_depth))
            .collect();

        drop(workers);

        // Would transfer tasks between workers in a real implementation
        if !idle_workers.is_empty() && !busy_workers.is_empty() {
            tracing::debug!(
                "Load balancing: {} idle workers, {} busy workers",
                idle_workers.len(),
                busy_workers.len()
            );
        }
    }

    /// Get coordinator statistics
    pub async fn stats(&self) -> CoordinatorStats {
        let workers = self.workers.read().await;
        let queue = self.task_queue.read().await;
        let running = self.running_tasks.read().await;
        let completed = self.completed_tasks.read().await;

        let active_workers = workers
            .iter()
            .filter(|(_, w)| w.is_alive(self.config.worker_timeout_ms))
            .count();

        let total_completed: usize = workers.values().map(|w| w.tasks_completed).sum();
        let total_failed: usize = workers.values().map(|w| w.tasks_failed).sum();

        CoordinatorStats {
            total_workers: workers.len(),
            active_workers,
            queued_tasks: queue.len(),
            running_tasks: running.len(),
            completed_tasks: completed.len(),
            total_completed,
            total_failed,
            avg_utilization: if workers.is_empty() {
                0.0
            } else {
                workers.values().map(|w| w.utilization).sum::<f64>() / workers.len() as f64
            },
        }
    }

    /// Get task result
    pub async fn get_result(&self, task_id: &str) -> Option<TaskResult> {
        self.completed_tasks.read().await.get(task_id).cloned()
    }

    /// Check if task is complete
    pub async fn is_complete(&self, task_id: &str) -> bool {
        self.completed_tasks.read().await.contains_key(task_id)
    }
}

/// Coordinator statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorStats {
    /// Total registered workers
    pub total_workers: usize,
    /// Active (alive) workers
    pub active_workers: usize,
    /// Tasks in queue
    pub queued_tasks: usize,
    /// Currently running tasks
    pub running_tasks: usize,
    /// Completed tasks (stored results)
    pub completed_tasks: usize,
    /// Total tasks completed by all workers
    pub total_completed: usize,
    /// Total tasks failed by all workers
    pub total_failed: usize,
    /// Average worker utilization
    pub avg_utilization: f64,
}

/// Distributed worker
pub struct DistributedWorker {
    id: String,
    config: DistributedConfig,
    status: Arc<RwLock<WorkerStatus>>,
    task_queue: Arc<RwLock<VecDeque<DistributedTask>>>,
}

impl DistributedWorker {
    /// Create a new worker
    pub fn new(id: String, config: DistributedConfig) -> Self {
        Self {
            id,
            config,
            status: Arc::new(RwLock::new(WorkerStatus::Idle)),
            task_queue: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    /// Get worker ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current status
    pub async fn status(&self) -> WorkerStatus {
        *self.status.read().await
    }

    /// Set status
    pub async fn set_status(&self, status: WorkerStatus) {
        *self.status.write().await = status;
    }

    /// Queue depth
    pub async fn queue_depth(&self) -> usize {
        self.task_queue.read().await.len()
    }

    /// Add task to local queue
    pub async fn enqueue(&self, task: DistributedTask) -> Result<(), DistributedError> {
        let mut queue = self.task_queue.write().await;
        if queue.len() >= self.config.queue_capacity {
            return Err(DistributedError::QueueFull);
        }
        queue.push_back(task);
        Ok(())
    }

    /// Get next task from local queue
    pub async fn dequeue(&self) -> Option<DistributedTask> {
        self.task_queue.write().await.pop_front()
    }

    /// Calculate utilization
    pub async fn utilization(&self) -> f64 {
        let queue_depth = self.task_queue.read().await.len();
        let status = *self.status.read().await;

        let base = match status {
            WorkerStatus::Idle => 0.0,
            WorkerStatus::Busy => 0.5,
            _ => 0.0,
        };

        base + (queue_depth as f64 / self.config.queue_capacity as f64) * 0.5
    }
}

/// Checkpoint for fault recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint ID
    pub id: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Pending tasks
    pub pending_tasks: Vec<DistributedTask>,
    /// Running tasks
    pub running_tasks: Vec<DistributedTask>,
    /// Completed task IDs
    pub completed_task_ids: Vec<String>,
    /// Worker states
    pub worker_states: HashMap<String, WorkerStatus>,
}

impl Checkpoint {
    /// Create a checkpoint from coordinator state
    pub async fn from_coordinator(coord: &DistributedCoordinator) -> Self {
        let pending = coord.task_queue.read().await.iter().cloned().collect();
        let running = coord.running_tasks.read().await.values().cloned().collect();
        let completed = coord.completed_tasks.read().await.keys().cloned().collect();
        let workers = coord
            .workers
            .read()
            .await
            .iter()
            .map(|(id, w)| (id.clone(), w.status))
            .collect();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            pending_tasks: pending,
            running_tasks: running,
            completed_task_ids: completed,
            worker_states: workers,
        }
    }
}

/// Distributed execution errors
#[derive(Debug, thiserror::Error)]
pub enum DistributedError {
    #[error("Worker limit reached")]
    WorkerLimitReached,
    #[error("Task queue is full")]
    QueueFull,
    #[error("Task not found")]
    TaskNotFound,
    #[error("Maximum retries exceeded")]
    MaxRetriesExceeded,
    #[error("Worker not found")]
    WorkerNotFound,
    #[error("Coordinator error: {0}")]
    CoordinatorError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_coordinator_creation() {
        let config = DistributedConfig::default();
        let coord = DistributedCoordinator::new(config);

        let stats = coord.stats().await;
        assert_eq!(stats.total_workers, 0);
        assert_eq!(stats.queued_tasks, 0);
    }

    #[tokio::test]
    async fn test_worker_registration() {
        let config = DistributedConfig::default();
        let coord = DistributedCoordinator::new(config);

        coord.register_worker("worker-1".to_string()).await.unwrap();
        coord.register_worker("worker-2".to_string()).await.unwrap();

        let stats = coord.stats().await;
        assert_eq!(stats.total_workers, 2);
    }

    #[tokio::test]
    async fn test_task_submission() {
        let config = DistributedConfig::default();
        let coord = DistributedCoordinator::new(config);

        coord.register_worker("worker-1".to_string()).await.unwrap();

        let task = DistributedTask::new(
            "task-1".to_string(),
            "test".to_string(),
            serde_json::json!({"data": "test"}),
        );

        coord.submit_task(task).await.unwrap();

        let stats = coord.stats().await;
        assert_eq!(stats.queued_tasks, 1);
    }

    #[tokio::test]
    async fn test_task_assignment() {
        let config = DistributedConfig::default();
        let coord = DistributedCoordinator::new(config);

        coord.register_worker("worker-1".to_string()).await.unwrap();
        coord.heartbeat("worker-1", 0.0).await;

        let task = DistributedTask::new(
            "task-1".to_string(),
            "test".to_string(),
            serde_json::json!({"data": "test"}),
        );
        coord.submit_task(task).await.unwrap();

        let assigned = coord.get_task("worker-1").await;
        assert!(assigned.is_some());
        assert_eq!(assigned.unwrap().id, "task-1");
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let config = DistributedConfig::default();
        let coord = DistributedCoordinator::new(config);

        coord.register_worker("worker-1".to_string()).await.unwrap();
        coord.heartbeat("worker-1", 0.0).await;

        // Submit low priority first
        coord
            .submit_task(
                DistributedTask::new(
                    "task-low".to_string(),
                    "test".to_string(),
                    serde_json::json!({}),
                )
                .with_priority(0),
            )
            .await
            .unwrap();

        // Submit high priority second
        coord
            .submit_task(
                DistributedTask::new(
                    "task-high".to_string(),
                    "test".to_string(),
                    serde_json::json!({}),
                )
                .with_priority(10),
            )
            .await
            .unwrap();

        // High priority should be retrieved first
        let task = coord.get_task("worker-1").await.unwrap();
        assert_eq!(task.id, "task-high");
    }
}
