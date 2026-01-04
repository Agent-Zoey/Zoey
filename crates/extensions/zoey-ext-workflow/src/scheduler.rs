/*!
# Scheduler

Provides scheduling capabilities for workflows.
*/

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Cron expression (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronExpression {
    /// Original expression
    pub expression: String,

    /// Minutes (0-59)
    pub minutes: Vec<u8>,

    /// Hours (0-23)
    pub hours: Vec<u8>,

    /// Days of month (1-31)
    pub days_of_month: Vec<u8>,

    /// Months (1-12)
    pub months: Vec<u8>,

    /// Days of week (0-6, 0=Sunday)
    pub days_of_week: Vec<u8>,
}

impl CronExpression {
    /// Parse a cron expression
    pub fn parse(expression: &str) -> Result<Self, SchedulerError> {
        let parts: Vec<&str> = expression.split_whitespace().collect();

        if parts.len() != 5 {
            return Err(SchedulerError::InvalidCron(
                "Expected 5 fields (minute hour day month weekday)".to_string(),
            ));
        }

        Ok(Self {
            expression: expression.to_string(),
            minutes: Self::parse_field(parts[0], 0, 59)?,
            hours: Self::parse_field(parts[1], 0, 23)?,
            days_of_month: Self::parse_field(parts[2], 1, 31)?,
            months: Self::parse_field(parts[3], 1, 12)?,
            days_of_week: Self::parse_field(parts[4], 0, 6)?,
        })
    }

    fn parse_field(field: &str, min: u8, max: u8) -> Result<Vec<u8>, SchedulerError> {
        if field == "*" {
            return Ok((min..=max).collect());
        }

        let mut values = Vec::new();

        for part in field.split(',') {
            if part.contains('/') {
                // Step values like */5
                let step_parts: Vec<&str> = part.split('/').collect();
                let step: u8 = step_parts[1]
                    .parse()
                    .map_err(|_| SchedulerError::InvalidCron(format!("Invalid step: {}", part)))?;

                let start = if step_parts[0] == "*" {
                    min
                } else {
                    step_parts[0].parse().map_err(|_| {
                        SchedulerError::InvalidCron(format!("Invalid start: {}", part))
                    })?
                };

                let mut current = start;
                while current <= max {
                    values.push(current);
                    current += step;
                }
            } else if part.contains('-') {
                // Range like 1-5
                let range_parts: Vec<&str> = part.split('-').collect();
                let start: u8 = range_parts[0].parse().map_err(|_| {
                    SchedulerError::InvalidCron(format!("Invalid range start: {}", part))
                })?;
                let end: u8 = range_parts[1].parse().map_err(|_| {
                    SchedulerError::InvalidCron(format!("Invalid range end: {}", part))
                })?;

                for v in start..=end {
                    values.push(v);
                }
            } else {
                // Single value
                let value: u8 = part
                    .parse()
                    .map_err(|_| SchedulerError::InvalidCron(format!("Invalid value: {}", part)))?;
                values.push(value);
            }
        }

        values.sort();
        values.dedup();
        Ok(values)
    }

    /// Check if the expression matches a given time
    pub fn matches(&self, time: &DateTime<Utc>) -> bool {
        let minute = time.format("%M").to_string().parse::<u8>().unwrap_or(0);
        let hour = time.format("%H").to_string().parse::<u8>().unwrap_or(0);
        let day = time.format("%d").to_string().parse::<u8>().unwrap_or(1);
        let month = time.format("%m").to_string().parse::<u8>().unwrap_or(1);
        let weekday = time.format("%w").to_string().parse::<u8>().unwrap_or(0);

        self.minutes.contains(&minute)
            && self.hours.contains(&hour)
            && self.days_of_month.contains(&day)
            && self.months.contains(&month)
            && self.days_of_week.contains(&weekday)
    }
}

/// Schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Cron expression
    pub cron: String,

    /// Timezone
    pub timezone: String,

    /// Enable schedule
    pub enabled: bool,

    /// Start date
    pub start_date: Option<DateTime<Utc>>,

    /// End date
    pub end_date: Option<DateTime<Utc>>,

    /// Maximum runs
    pub max_runs: Option<usize>,

    /// Catch up missed runs
    pub catch_up: bool,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            cron: "0 * * * *".to_string(), // Every hour
            timezone: "UTC".to_string(),
            enabled: true,
            start_date: None,
            end_date: None,
            max_runs: None,
            catch_up: false,
        }
    }
}

/// A scheduled job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    /// Job ID
    pub id: Uuid,

    /// Job name
    pub name: String,

    /// Workflow ID to run
    pub workflow_id: Uuid,

    /// Schedule configuration
    pub config: ScheduleConfig,

    /// Cron expression (parsed)
    #[serde(skip)]
    pub cron: Option<CronExpression>,

    /// Run count
    pub run_count: usize,

    /// Last run time
    pub last_run: Option<DateTime<Utc>>,

    /// Next run time
    pub next_run: Option<DateTime<Utc>>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

impl ScheduledJob {
    /// Create a new scheduled job
    pub fn new(
        name: impl Into<String>,
        workflow_id: Uuid,
        config: ScheduleConfig,
    ) -> Result<Self, SchedulerError> {
        let cron = CronExpression::parse(&config.cron)?;

        let mut job = Self {
            id: Uuid::new_v4(),
            name: name.into(),
            workflow_id,
            config,
            cron: Some(cron),
            run_count: 0,
            last_run: None,
            next_run: None,
            created_at: Utc::now(),
        };

        job.compute_next_run();
        Ok(job)
    }

    /// Compute next run time
    pub fn compute_next_run(&mut self) {
        let start = self.last_run.unwrap_or_else(Utc::now);
        let cron = match &self.cron {
            Some(c) => c,
            None => return,
        };

        // Simple implementation: check each minute for next 24 hours
        let mut check_time = start + Duration::minutes(1);
        for _ in 0..(24 * 60) {
            if cron.matches(&check_time) {
                // Check constraints
                if let Some(end) = self.config.end_date {
                    if check_time > end {
                        self.next_run = None;
                        return;
                    }
                }
                if let Some(max) = self.config.max_runs {
                    if self.run_count >= max {
                        self.next_run = None;
                        return;
                    }
                }
                self.next_run = Some(check_time);
                return;
            }
            check_time = check_time + Duration::minutes(1);
        }

        self.next_run = None;
    }

    /// Record a run
    pub fn record_run(&mut self) {
        self.run_count += 1;
        self.last_run = Some(Utc::now());
        self.compute_next_run();
    }

    /// Check if job should run now
    pub fn should_run(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        if let Some(next) = self.next_run {
            Utc::now() >= next
        } else {
            false
        }
    }
}

/// The scheduler
pub struct Scheduler {
    jobs: Arc<RwLock<HashMap<Uuid, ScheduledJob>>>,
    running: Arc<RwLock<bool>>,
}

impl Scheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Schedule a workflow
    pub async fn schedule(
        &self,
        name: impl Into<String>,
        workflow_id: Uuid,
        config: ScheduleConfig,
    ) -> Result<Uuid, SchedulerError> {
        let job = ScheduledJob::new(name, workflow_id, config)?;
        let job_id = job.id;

        self.jobs.write().await.insert(job_id, job);
        tracing::info!("Scheduled job: {}", job_id);

        Ok(job_id)
    }

    /// Schedule with cron expression
    pub async fn schedule_cron(
        &self,
        name: impl Into<String>,
        workflow_id: Uuid,
        cron: &str,
    ) -> Result<Uuid, SchedulerError> {
        let config = ScheduleConfig {
            cron: cron.to_string(),
            ..Default::default()
        };
        self.schedule(name, workflow_id, config).await
    }

    /// Unschedule a job
    pub async fn unschedule(&self, job_id: Uuid) -> Option<ScheduledJob> {
        self.jobs.write().await.remove(&job_id)
    }

    /// Get job by ID
    pub async fn get_job(&self, job_id: Uuid) -> Option<ScheduledJob> {
        self.jobs.read().await.get(&job_id).cloned()
    }

    /// List all jobs
    pub async fn list_jobs(&self) -> Vec<ScheduledJob> {
        self.jobs.read().await.values().cloned().collect()
    }

    /// Get jobs due to run
    pub async fn get_due_jobs(&self) -> Vec<ScheduledJob> {
        self.jobs
            .read()
            .await
            .values()
            .filter(|j| j.should_run())
            .cloned()
            .collect()
    }

    /// Pause a job
    pub async fn pause(&self, job_id: Uuid) -> bool {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.config.enabled = false;
            true
        } else {
            false
        }
    }

    /// Resume a job
    pub async fn resume(&self, job_id: Uuid) -> bool {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.config.enabled = true;
            job.compute_next_run();
            true
        } else {
            false
        }
    }

    /// Record job execution
    pub async fn record_execution(&self, job_id: Uuid) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.record_run();
        }
    }

    /// Start scheduler loop
    pub async fn start(&self) {
        *self.running.write().await = true;
        tracing::info!("Scheduler started");
    }

    /// Stop scheduler
    pub async fn stop(&self) {
        *self.running.write().await = false;
        tracing::info!("Scheduler stopped");
    }

    /// Check if running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Scheduler errors
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Invalid cron expression: {0}")]
    InvalidCron(String),

    #[error("Job not found: {0}")]
    JobNotFound(Uuid),

    #[error("Schedule conflict: {0}")]
    Conflict(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_parse() {
        let cron = CronExpression::parse("0 * * * *").unwrap();
        assert_eq!(cron.minutes, vec![0]);
        assert_eq!(cron.hours.len(), 24);
    }

    #[test]
    fn test_cron_parse_step() {
        let cron = CronExpression::parse("*/15 * * * *").unwrap();
        assert_eq!(cron.minutes, vec![0, 15, 30, 45]);
    }

    #[test]
    fn test_cron_parse_range() {
        let cron = CronExpression::parse("0 9-17 * * *").unwrap();
        assert_eq!(cron.hours, vec![9, 10, 11, 12, 13, 14, 15, 16, 17]);
    }

    #[tokio::test]
    async fn test_scheduler() {
        let scheduler = Scheduler::new();

        let job_id = scheduler
            .schedule_cron("test_job", Uuid::new_v4(), "*/5 * * * *")
            .await
            .unwrap();

        let job = scheduler.get_job(job_id).await.unwrap();
        assert!(job.next_run.is_some());

        let jobs = scheduler.list_jobs().await;
        assert_eq!(jobs.len(), 1);
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let scheduler = Scheduler::new();

        let job_id = scheduler
            .schedule_cron("test", Uuid::new_v4(), "0 * * * *")
            .await
            .unwrap();

        scheduler.pause(job_id).await;
        let job = scheduler.get_job(job_id).await.unwrap();
        assert!(!job.config.enabled);

        scheduler.resume(job_id).await;
        let job = scheduler.get_job(job_id).await.unwrap();
        assert!(job.config.enabled);
    }
}
