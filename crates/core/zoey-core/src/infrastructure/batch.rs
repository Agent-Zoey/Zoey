//! Batch processing API
//!
//! Provides efficient bulk operations for:
//! - Message processing
//! - Memory operations
//! - Data migration

use crate::Result;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Semaphore};

/// Batch processing options
#[derive(Debug, Clone)]
pub struct BatchOptions {
    /// Maximum concurrent operations
    pub concurrency: usize,
    /// Stop on first error
    pub fail_fast: bool,
    /// Timeout per item
    pub item_timeout: Duration,
    /// Overall batch timeout
    pub batch_timeout: Option<Duration>,
    /// Progress reporting interval
    pub progress_interval: Duration,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            concurrency: 10,
            fail_fast: false,
            item_timeout: Duration::from_secs(30),
            batch_timeout: None,
            progress_interval: Duration::from_secs(1),
        }
    }
}

/// Progress information for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgress {
    /// Total items to process
    pub total: u64,
    /// Items completed
    pub completed: u64,
    /// Items failed
    pub failed: u64,
    /// Items in progress
    pub in_progress: u64,
    /// Elapsed time in milliseconds
    pub elapsed_ms: u64,
    /// Estimated remaining time in milliseconds
    pub estimated_remaining_ms: Option<u64>,
    /// Items per second
    pub items_per_second: f64,
}

impl BatchProgress {
    /// Calculate percentage complete
    pub fn percentage(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            (self.completed as f64 / self.total as f64) * 100.0
        }
    }
}

/// Result of a single batch item
#[derive(Debug, Clone)]
pub enum BatchItemResult<T> {
    /// Item processed successfully
    Success(T),
    /// Item processing failed
    Failure(String),
    /// Item was skipped
    Skipped(String),
}

/// Result of batch processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult<T> {
    /// Successful results
    pub successes: Vec<T>,
    /// Failed item indices and errors
    pub failures: Vec<(usize, String)>,
    /// Skipped item indices and reasons
    pub skipped: Vec<(usize, String)>,
    /// Total processing time in milliseconds
    pub total_time_ms: u64,
    /// Whether batch was cancelled
    pub cancelled: bool,
}

impl<T> BatchResult<T> {
    /// Check if all items succeeded
    pub fn is_all_success(&self) -> bool {
        self.failures.is_empty() && !self.cancelled
    }

    /// Get success count
    pub fn success_count(&self) -> usize {
        self.successes.len()
    }

    /// Get failure count
    pub fn failure_count(&self) -> usize {
        self.failures.len()
    }
}

/// Batch processor for executing operations in bulk
pub struct BatchProcessor {
    options: BatchOptions,
    cancelled: Arc<AtomicBool>,
}

impl BatchProcessor {
    /// Create a new batch processor
    pub fn new(options: BatchOptions) -> Self {
        Self {
            options,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create with default options
    pub fn with_defaults() -> Self {
        Self::new(BatchOptions::default())
    }

    /// Cancel the batch operation
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Process items in batch
    pub async fn process<T, R, F, Fut>(
        &self,
        items: Vec<T>,
        f: F,
        progress_callback: Option<mpsc::Sender<BatchProgress>>,
    ) -> BatchResult<R>
    where
        T: Send + 'static,
        R: Send + 'static,
        F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<R>> + Send,
    {
        let start = Instant::now();
        let total = items.len() as u64;

        let completed = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicU64::new(0));
        let in_progress = Arc::new(AtomicU64::new(0));

        let semaphore = Arc::new(Semaphore::new(self.options.concurrency));
        let cancelled = Arc::clone(&self.cancelled);
        let fail_fast = self.options.fail_fast;

        // Progress reporter task
        let progress_completed = Arc::clone(&completed);
        let progress_failed = Arc::clone(&failed);
        let progress_in_progress = Arc::clone(&in_progress);
        let progress_interval = self.options.progress_interval;

        if let Some(tx) = progress_callback.as_ref() {
            let tx = tx.clone();
            let cancelled = Arc::clone(&cancelled);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(progress_interval);
                loop {
                    interval.tick().await;

                    if cancelled.load(Ordering::SeqCst) {
                        break;
                    }

                    let comp = progress_completed.load(Ordering::SeqCst);
                    let fail = progress_failed.load(Ordering::SeqCst);
                    let prog = progress_in_progress.load(Ordering::SeqCst);
                    let elapsed = start.elapsed().as_millis() as u64;

                    let items_per_second = if elapsed > 0 {
                        (comp as f64 / elapsed as f64) * 1000.0
                    } else {
                        0.0
                    };

                    let remaining = total.saturating_sub(comp + fail);
                    let estimated_remaining_ms = if items_per_second > 0.0 {
                        Some((remaining as f64 / items_per_second * 1000.0) as u64)
                    } else {
                        None
                    };

                    let progress = BatchProgress {
                        total,
                        completed: comp,
                        failed: fail,
                        in_progress: prog,
                        elapsed_ms: elapsed,
                        estimated_remaining_ms,
                        items_per_second,
                    };

                    if tx.send(progress).await.is_err() {
                        break;
                    }

                    if comp + fail >= total {
                        break;
                    }
                }
            });
        }

        // Process items
        let mut handles = Vec::with_capacity(items.len());

        for (idx, item) in items.into_iter().enumerate() {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let f = f.clone();
            let completed = Arc::clone(&completed);
            let failed = Arc::clone(&failed);
            let in_progress = Arc::clone(&in_progress);
            let cancelled = Arc::clone(&cancelled);
            let timeout = self.options.item_timeout;

            in_progress.fetch_add(1, Ordering::SeqCst);

            let handle = tokio::spawn(async move {
                let _permit = permit;

                // Check cancellation
                if cancelled.load(Ordering::SeqCst) {
                    in_progress.fetch_sub(1, Ordering::SeqCst);
                    return (idx, BatchItemResult::Skipped("Cancelled".to_string()));
                }

                // Execute with timeout
                let result = match tokio::time::timeout(timeout, f(item)).await {
                    Ok(Ok(value)) => {
                        completed.fetch_add(1, Ordering::SeqCst);
                        BatchItemResult::Success(value)
                    }
                    Ok(Err(e)) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        if fail_fast {
                            cancelled.store(true, Ordering::SeqCst);
                        }
                        BatchItemResult::Failure(e.to_string())
                    }
                    Err(_) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        BatchItemResult::Failure("Timeout".to_string())
                    }
                };

                in_progress.fetch_sub(1, Ordering::SeqCst);
                (idx, result)
            });

            handles.push(handle);
        }

        // Collect results
        let mut successes = Vec::new();
        let mut failures = Vec::new();
        let mut skipped = Vec::new();

        for handle in handles {
            match handle.await {
                Ok((_idx, BatchItemResult::Success(value))) => {
                    successes.push(value);
                }
                Ok((idx, BatchItemResult::Failure(err))) => {
                    failures.push((idx, err));
                }
                Ok((idx, BatchItemResult::Skipped(reason))) => {
                    skipped.push((idx, reason));
                }
                Err(e) => {
                    // Task panicked
                    failures.push((0, format!("Task panic: {}", e)));
                }
            }
        }

        BatchResult {
            successes,
            failures,
            skipped,
            total_time_ms: start.elapsed().as_millis() as u64,
            cancelled: self.cancelled.load(Ordering::SeqCst),
        }
    }
}

/// Batch message request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMessageRequest {
    /// Messages to process
    pub messages: Vec<BatchMessage>,
    /// Processing options
    #[serde(default)]
    pub options: BatchMessageOptions,
}

/// Single message in batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMessage {
    /// Message ID (optional, will be generated if not provided)
    pub id: Option<String>,
    /// Message content
    pub content: String,
    /// Room ID
    pub room_id: String,
    /// Entity ID (sender)
    pub entity_id: String,
    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Batch message processing options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BatchMessageOptions {
    /// Maximum concurrent message processing
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// Stop on first error
    #[serde(default)]
    pub fail_fast: bool,
}

fn default_concurrency() -> usize {
    5
}

/// Batch message response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMessageResponse {
    /// Successful message results
    pub results: Vec<BatchMessageResult>,
    /// Failed messages
    pub errors: Vec<BatchMessageError>,
    /// Processing statistics
    pub stats: BatchStats,
}

/// Result of a single message in batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMessageResult {
    /// Original message ID
    pub message_id: String,
    /// Response content
    pub response: String,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Error for a single message in batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMessageError {
    /// Original message ID
    pub message_id: String,
    /// Error message
    pub error: String,
}

/// Batch processing statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStats {
    /// Total messages
    pub total: usize,
    /// Successful messages
    pub successful: usize,
    /// Failed messages
    pub failed: usize,
    /// Total processing time in milliseconds
    pub total_time_ms: u64,
    /// Average processing time per message
    pub avg_time_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ZoeyError;

    #[tokio::test]
    async fn test_batch_processor_success() {
        let processor = BatchProcessor::with_defaults();
        let items = vec![1, 2, 3, 4, 5];

        let result = processor
            .process(items, |x| async move { Ok(x * 2) }, None)
            .await;

        assert!(result.is_all_success());
        assert_eq!(result.success_count(), 5);
        assert!(result.successes.contains(&2));
        assert!(result.successes.contains(&10));
    }

    #[tokio::test]
    async fn test_batch_processor_with_failures() {
        let processor = BatchProcessor::with_defaults();
        let items = vec![1, 2, 3, 4, 5];

        let result = processor
            .process(
                items,
                |x| async move {
                    if x == 3 {
                        Err(ZoeyError::other("Test error"))
                    } else {
                        Ok(x * 2)
                    }
                },
                None,
            )
            .await;

        assert!(!result.is_all_success());
        assert_eq!(result.success_count(), 4);
        assert_eq!(result.failure_count(), 1);
    }

    #[tokio::test]
    async fn test_batch_processor_fail_fast() {
        let options = BatchOptions {
            fail_fast: true,
            concurrency: 1, // Sequential to ensure deterministic order
            ..Default::default()
        };
        let processor = BatchProcessor::new(options);
        let items = vec![1, 2, 3, 4, 5];

        let result = processor
            .process(
                items,
                |x| async move {
                    if x == 2 {
                        Err(ZoeyError::other("Test error"))
                    } else {
                        Ok(x * 2)
                    }
                },
                None,
            )
            .await;

        // Should have stopped after first failure
        assert!(result.cancelled);
    }

    #[tokio::test]
    async fn test_batch_progress_calculation() {
        let progress = BatchProgress {
            total: 100,
            completed: 50,
            failed: 10,
            in_progress: 5,
            elapsed_ms: 1000,
            estimated_remaining_ms: Some(800),
            items_per_second: 50.0,
        };

        assert_eq!(progress.percentage(), 50.0);
    }
}
