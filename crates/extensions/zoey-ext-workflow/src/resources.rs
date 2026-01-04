/*!
# Resource Management

Provides resource constraints and management for workflow execution including:
- CPU, memory, GPU resource allocation
- Resource quotas and limits
- Resource pools and reservation
- Backpressure and throttling
*/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};

/// Resource types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    /// CPU cores
    Cpu,
    /// Memory in bytes
    Memory,
    /// GPU units
    Gpu,
    /// Network bandwidth (bytes/sec)
    Network,
    /// Disk I/O (operations/sec)
    DiskIO,
    /// Custom resource
    Custom,
}

/// Resource requirements for a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// Required CPU (cores or millicores)
    pub cpu: Option<f64>,
    /// Required memory (bytes)
    pub memory: Option<usize>,
    /// Required GPU units
    pub gpu: Option<f64>,
    /// Required network bandwidth
    pub network: Option<usize>,
    /// Required disk I/O
    pub disk_io: Option<usize>,
    /// Custom resources
    pub custom: HashMap<String, f64>,
    /// Resource priority (higher = more important)
    pub priority: i32,
    /// Allow preemption by higher priority tasks
    pub preemptible: bool,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            cpu: None,
            memory: None,
            gpu: None,
            network: None,
            disk_io: None,
            custom: HashMap::new(),
            priority: 0,
            preemptible: false,
        }
    }
}

impl ResourceRequirements {
    /// Create with CPU requirement
    pub fn with_cpu(mut self, cores: f64) -> Self {
        self.cpu = Some(cores);
        self
    }

    /// Create with memory requirement
    pub fn with_memory(mut self, bytes: usize) -> Self {
        self.memory = Some(bytes);
        self
    }

    /// Create with GPU requirement
    pub fn with_gpu(mut self, units: f64) -> Self {
        self.gpu = Some(units);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set preemptible
    pub fn preemptible(mut self) -> Self {
        self.preemptible = true;
        self
    }
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum CPU
    pub max_cpu: f64,
    /// Maximum memory (bytes)
    pub max_memory: usize,
    /// Maximum GPU
    pub max_gpu: f64,
    /// Maximum network bandwidth
    pub max_network: usize,
    /// Maximum disk I/O
    pub max_disk_io: usize,
    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,
    /// Maximum queue size
    pub max_queue_size: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu: 8.0,
            max_memory: 16 * 1024 * 1024 * 1024, // 16 GB
            max_gpu: 2.0,
            max_network: 1024 * 1024 * 1024, // 1 GB/s
            max_disk_io: 10000,
            max_concurrent_tasks: 100,
            max_queue_size: 1000,
        }
    }
}

/// Current resource usage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Used CPU
    pub cpu: f64,
    /// Used memory
    pub memory: usize,
    /// Used GPU
    pub gpu: f64,
    /// Used network
    pub network: usize,
    /// Used disk I/O
    pub disk_io: usize,
    /// Active tasks
    pub active_tasks: usize,
    /// Queued tasks
    pub queued_tasks: usize,
}

impl ResourceUsage {
    /// Check if requirements can be satisfied
    pub fn can_allocate(&self, req: &ResourceRequirements, limits: &ResourceLimits) -> bool {
        if let Some(cpu) = req.cpu {
            if self.cpu + cpu > limits.max_cpu {
                return false;
            }
        }
        if let Some(mem) = req.memory {
            if self.memory + mem > limits.max_memory {
                return false;
            }
        }
        if let Some(gpu) = req.gpu {
            if self.gpu + gpu > limits.max_gpu {
                return false;
            }
        }
        if self.active_tasks >= limits.max_concurrent_tasks {
            return false;
        }
        true
    }

    /// Allocate resources
    pub fn allocate(&mut self, req: &ResourceRequirements) {
        if let Some(cpu) = req.cpu {
            self.cpu += cpu;
        }
        if let Some(mem) = req.memory {
            self.memory += mem;
        }
        if let Some(gpu) = req.gpu {
            self.gpu += gpu;
        }
        self.active_tasks += 1;
    }

    /// Release resources
    pub fn release(&mut self, req: &ResourceRequirements) {
        if let Some(cpu) = req.cpu {
            self.cpu = (self.cpu - cpu).max(0.0);
        }
        if let Some(mem) = req.memory {
            self.memory = self.memory.saturating_sub(mem);
        }
        if let Some(gpu) = req.gpu {
            self.gpu = (self.gpu - gpu).max(0.0);
        }
        self.active_tasks = self.active_tasks.saturating_sub(1);
    }
}

/// Resource allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAllocation {
    /// Allocation ID
    pub id: String,
    /// Task ID
    pub task_id: String,
    /// Allocated resources
    pub resources: ResourceRequirements,
    /// Allocation time
    pub allocated_at: chrono::DateTime<chrono::Utc>,
    /// Expiration (if any)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Resource pool for managing allocations
pub struct ResourcePool {
    limits: ResourceLimits,
    usage: Arc<RwLock<ResourceUsage>>,
    allocations: Arc<RwLock<HashMap<String, ResourceAllocation>>>,
    waiting_queue: Arc<RwLock<Vec<PendingAllocation>>>,
    task_semaphore: Arc<Semaphore>,
}

/// Pending allocation request
#[derive(Debug)]
struct PendingAllocation {
    task_id: String,
    requirements: ResourceRequirements,
    queued_at: Instant,
}

impl ResourcePool {
    /// Create a new resource pool
    pub fn new(limits: ResourceLimits) -> Self {
        let max_tasks = limits.max_concurrent_tasks;
        Self {
            limits,
            usage: Arc::new(RwLock::new(ResourceUsage::default())),
            allocations: Arc::new(RwLock::new(HashMap::new())),
            waiting_queue: Arc::new(RwLock::new(Vec::new())),
            task_semaphore: Arc::new(Semaphore::new(max_tasks)),
        }
    }

    /// Try to allocate resources
    pub async fn allocate(
        &self,
        task_id: &str,
        requirements: ResourceRequirements,
    ) -> Result<ResourceAllocation, ResourceError> {
        let mut usage = self.usage.write().await;

        if !usage.can_allocate(&requirements, &self.limits) {
            return Err(ResourceError::InsufficientResources);
        }

        usage.allocate(&requirements);

        let allocation = ResourceAllocation {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            resources: requirements,
            allocated_at: chrono::Utc::now(),
            expires_at: None,
        };

        self.allocations
            .write()
            .await
            .insert(allocation.id.clone(), allocation.clone());

        Ok(allocation)
    }

    /// Release allocated resources
    pub async fn release(&self, allocation_id: &str) -> Result<(), ResourceError> {
        let mut allocations = self.allocations.write().await;
        let allocation = allocations
            .remove(allocation_id)
            .ok_or(ResourceError::AllocationNotFound)?;

        self.usage.write().await.release(&allocation.resources);
        Ok(())
    }

    /// Queue allocation request for later
    pub async fn queue_allocation(
        &self,
        task_id: &str,
        requirements: ResourceRequirements,
    ) -> Result<(), ResourceError> {
        let mut queue = self.waiting_queue.write().await;
        if queue.len() >= self.limits.max_queue_size {
            return Err(ResourceError::QueueFull);
        }

        let pending = PendingAllocation {
            task_id: task_id.to_string(),
            requirements,
            queued_at: Instant::now(),
        };

        // Insert by priority (higher priority first)
        let pos = queue
            .iter()
            .position(|p| p.requirements.priority < pending.requirements.priority);
        match pos {
            Some(idx) => queue.insert(idx, pending),
            None => queue.push(pending),
        }

        Ok(())
    }

    /// Process waiting queue
    pub async fn process_queue(&self) -> Vec<ResourceAllocation> {
        let mut queue = self.waiting_queue.write().await;
        let mut allocated = Vec::new();

        // Try to allocate from head of queue
        let mut i = 0;
        while i < queue.len() {
            let pending = &queue[i];
            let usage = self.usage.read().await;

            if usage.can_allocate(&pending.requirements, &self.limits) {
                drop(usage);
                let pending = queue.remove(i);
                if let Ok(alloc) = self.allocate(&pending.task_id, pending.requirements).await {
                    allocated.push(alloc);
                }
            } else {
                i += 1;
            }
        }

        allocated
    }

    /// Get current usage
    pub async fn usage(&self) -> ResourceUsage {
        self.usage.read().await.clone()
    }

    /// Get utilization percentages
    pub async fn utilization(&self) -> ResourceUtilization {
        let usage = self.usage.read().await;
        ResourceUtilization {
            cpu: (usage.cpu / self.limits.max_cpu * 100.0).min(100.0),
            memory: (usage.memory as f64 / self.limits.max_memory as f64 * 100.0).min(100.0),
            gpu: (usage.gpu / self.limits.max_gpu * 100.0).min(100.0),
            tasks: (usage.active_tasks as f64 / self.limits.max_concurrent_tasks as f64 * 100.0)
                .min(100.0),
        }
    }

    /// Check if pool is under pressure
    pub async fn is_under_pressure(&self, threshold: f64) -> bool {
        let util = self.utilization().await;
        util.cpu > threshold
            || util.memory > threshold
            || util.gpu > threshold
            || util.tasks > threshold
    }
}

/// Resource utilization percentages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilization {
    /// CPU utilization (%)
    pub cpu: f64,
    /// Memory utilization (%)
    pub memory: f64,
    /// GPU utilization (%)
    pub gpu: f64,
    /// Task slot utilization (%)
    pub tasks: f64,
}

/// Resource quota for a workflow or user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    /// Quota name
    pub name: String,
    /// CPU quota
    pub cpu_limit: f64,
    /// Memory quota
    pub memory_limit: usize,
    /// GPU quota
    pub gpu_limit: f64,
    /// Task quota
    pub task_limit: usize,
    /// Current CPU usage
    pub cpu_used: f64,
    /// Current memory usage
    pub memory_used: usize,
    /// Current GPU usage
    pub gpu_used: f64,
    /// Current task count
    pub tasks_used: usize,
}

impl ResourceQuota {
    /// Create new quota
    pub fn new(name: String, limits: ResourceLimits) -> Self {
        Self {
            name,
            cpu_limit: limits.max_cpu,
            memory_limit: limits.max_memory,
            gpu_limit: limits.max_gpu,
            task_limit: limits.max_concurrent_tasks,
            cpu_used: 0.0,
            memory_used: 0,
            gpu_used: 0.0,
            tasks_used: 0,
        }
    }

    /// Check if allocation would exceed quota
    pub fn would_exceed(&self, req: &ResourceRequirements) -> bool {
        if let Some(cpu) = req.cpu {
            if self.cpu_used + cpu > self.cpu_limit {
                return true;
            }
        }
        if let Some(mem) = req.memory {
            if self.memory_used + mem > self.memory_limit {
                return true;
            }
        }
        if let Some(gpu) = req.gpu {
            if self.gpu_used + gpu > self.gpu_limit {
                return true;
            }
        }
        if self.tasks_used >= self.task_limit {
            return true;
        }
        false
    }

    /// Consume quota
    pub fn consume(&mut self, req: &ResourceRequirements) {
        if let Some(cpu) = req.cpu {
            self.cpu_used += cpu;
        }
        if let Some(mem) = req.memory {
            self.memory_used += mem;
        }
        if let Some(gpu) = req.gpu {
            self.gpu_used += gpu;
        }
        self.tasks_used += 1;
    }

    /// Release quota
    pub fn release(&mut self, req: &ResourceRequirements) {
        if let Some(cpu) = req.cpu {
            self.cpu_used = (self.cpu_used - cpu).max(0.0);
        }
        if let Some(mem) = req.memory {
            self.memory_used = self.memory_used.saturating_sub(mem);
        }
        if let Some(gpu) = req.gpu {
            self.gpu_used = (self.gpu_used - gpu).max(0.0);
        }
        self.tasks_used = self.tasks_used.saturating_sub(1);
    }
}

/// Throttle configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThrottleConfig {
    /// Requests per second limit
    pub rate_limit: f64,
    /// Burst size
    pub burst_size: usize,
    /// Enable backpressure
    pub backpressure: bool,
    /// Backpressure threshold (utilization %)
    pub backpressure_threshold: f64,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            rate_limit: 100.0,
            burst_size: 10,
            backpressure: true,
            backpressure_threshold: 80.0,
        }
    }
}

/// Rate limiter using token bucket
pub struct RateLimiter {
    config: ThrottleConfig,
    tokens: Arc<RwLock<f64>>,
    last_refill: Arc<RwLock<Instant>>,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(config: ThrottleConfig) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(config.burst_size as f64)),
            last_refill: Arc::new(RwLock::new(Instant::now())),
            config,
        }
    }

    /// Try to acquire a permit
    pub async fn try_acquire(&self) -> bool {
        self.refill().await;

        let mut tokens = self.tokens.write().await;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Wait for permit (with timeout)
    pub async fn acquire(&self, timeout: Duration) -> Result<(), ResourceError> {
        let start = Instant::now();
        loop {
            if self.try_acquire().await {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(ResourceError::RateLimited);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Refill tokens based on elapsed time
    async fn refill(&self) {
        let mut last = self.last_refill.write().await;
        let elapsed = last.elapsed();
        *last = Instant::now();

        let new_tokens = elapsed.as_secs_f64() * self.config.rate_limit;
        let mut tokens = self.tokens.write().await;
        *tokens = (*tokens + new_tokens).min(self.config.burst_size as f64);
    }

    /// Get current token count
    pub async fn available(&self) -> f64 {
        *self.tokens.read().await
    }
}

/// Resource errors
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("Insufficient resources")]
    InsufficientResources,
    #[error("Allocation not found")]
    AllocationNotFound,
    #[error("Queue is full")]
    QueueFull,
    #[error("Rate limited")]
    RateLimited,
    #[error("Quota exceeded")]
    QuotaExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resource_allocation() {
        let limits = ResourceLimits {
            max_cpu: 4.0,
            max_memory: 8 * 1024 * 1024 * 1024,
            max_concurrent_tasks: 10,
            ..Default::default()
        };
        let pool = ResourcePool::new(limits);

        let req = ResourceRequirements::default()
            .with_cpu(2.0)
            .with_memory(4 * 1024 * 1024 * 1024);

        let alloc = pool.allocate("task-1", req).await.unwrap();
        assert_eq!(alloc.task_id, "task-1");

        let usage = pool.usage().await;
        assert_eq!(usage.cpu, 2.0);
        assert_eq!(usage.active_tasks, 1);
    }

    #[tokio::test]
    async fn test_insufficient_resources() {
        let limits = ResourceLimits {
            max_cpu: 2.0,
            ..Default::default()
        };
        let pool = ResourcePool::new(limits);

        let req = ResourceRequirements::default().with_cpu(3.0);
        let result = pool.allocate("task-1", req).await;

        assert!(matches!(result, Err(ResourceError::InsufficientResources)));
    }

    #[tokio::test]
    async fn test_utilization() {
        let limits = ResourceLimits {
            max_cpu: 4.0,
            max_memory: 8 * 1024 * 1024 * 1024,
            max_concurrent_tasks: 10,
            ..Default::default()
        };
        let pool = ResourcePool::new(limits);

        let req = ResourceRequirements::default().with_cpu(2.0);
        pool.allocate("task-1", req).await.unwrap();

        let util = pool.utilization().await;
        assert_eq!(util.cpu, 50.0);
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let config = ThrottleConfig {
            rate_limit: 10.0,
            burst_size: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Should allow burst
        for _ in 0..5 {
            assert!(limiter.try_acquire().await);
        }

        // Should be rate limited
        assert!(!limiter.try_acquire().await);
    }

    #[test]
    fn test_quota() {
        let limits = ResourceLimits {
            max_cpu: 4.0,
            max_concurrent_tasks: 2,
            ..Default::default()
        };
        let mut quota = ResourceQuota::new("test".to_string(), limits);

        let req = ResourceRequirements::default().with_cpu(2.0);
        assert!(!quota.would_exceed(&req));
        quota.consume(&req);

        assert!(!quota.would_exceed(&req));
        quota.consume(&req);

        // Now should exceed task limit
        assert!(quota.would_exceed(&req));
    }
}
