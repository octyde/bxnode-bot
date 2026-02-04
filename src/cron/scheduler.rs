//! Cron scheduler implementation

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

/// A scheduled cron job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    /// Unique job identifier
    pub id: String,

    /// Cron expression (e.g., "0 0 * * *" for daily at midnight)
    pub schedule: String,

    /// Job payload (action to execute)
    pub payload: serde_json::Value,

    /// Whether the job is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Last execution time
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,

    /// Last execution result
    #[serde(default)]
    pub last_result: Option<CronJobResult>,
}

fn default_enabled() -> bool {
    true
}

/// Result of a cron job execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobResult {
    /// Whether the execution succeeded
    pub success: bool,

    /// Execution time
    pub executed_at: DateTime<Utc>,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Error message if failed
    pub error: Option<String>,
}

/// Event emitted when a job is due to run
#[derive(Debug, Clone)]
pub struct CronEvent {
    pub job_id: String,
    pub payload: serde_json::Value,
    pub scheduled_at: DateTime<Utc>,
}

/// Cron scheduler
pub struct CronScheduler {
    jobs: Arc<RwLock<HashMap<String, CronJob>>>,
    event_tx: mpsc::Sender<CronEvent>,
    event_rx: RwLock<Option<mpsc::Receiver<CronEvent>>>,
    shutdown_tx: RwLock<Option<tokio::sync::oneshot::Sender<()>>>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl CronScheduler {
    /// Create a new cron scheduler
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: RwLock::new(Some(event_rx)),
            shutdown_tx: RwLock::new(None),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Take the event receiver (can only be called once)
    pub async fn take_event_receiver(&self) -> Option<mpsc::Receiver<CronEvent>> {
        self.event_rx.write().await.take()
    }

    /// Add a job to the scheduler
    pub async fn add_job(&self, job: CronJob) -> anyhow::Result<()> {
        // Validate cron expression
        let _ = job.schedule.parse::<Schedule>().map_err(|e| {
            anyhow::anyhow!("Invalid cron expression '{}': {}", job.schedule, e)
        })?;

        let mut jobs = self.jobs.write().await;
        jobs.insert(job.id.clone(), job);
        Ok(())
    }

    /// Remove a job from the scheduler
    pub async fn remove_job(&self, id: &str) -> Option<CronJob> {
        let mut jobs = self.jobs.write().await;
        jobs.remove(id)
    }

    /// Get a job by ID
    pub async fn get_job(&self, id: &str) -> Option<CronJob> {
        let jobs = self.jobs.read().await;
        jobs.get(id).cloned()
    }

    /// List all jobs
    pub async fn list_jobs(&self) -> Vec<CronJob> {
        let jobs = self.jobs.read().await;
        jobs.values().cloned().collect()
    }

    /// Update a job
    pub async fn update_job(&self, job: CronJob) -> anyhow::Result<bool> {
        // Validate cron expression
        let _ = job.schedule.parse::<Schedule>().map_err(|e| {
            anyhow::anyhow!("Invalid cron expression '{}': {}", job.schedule, e)
        })?;

        let mut jobs = self.jobs.write().await;
        if jobs.contains_key(&job.id) {
            jobs.insert(job.id.clone(), job);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Run a job immediately (manual trigger)
    pub async fn run_job(&self, id: &str) -> anyhow::Result<()> {
        let jobs = self.jobs.read().await;
        let job = jobs
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", id))?;

        let event = CronEvent {
            job_id: job.id.clone(),
            payload: job.payload.clone(),
            scheduled_at: Utc::now(),
        };

        self.event_tx
            .send(event)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to emit job event: {}", e))?;

        Ok(())
    }

    /// Record job execution result
    pub async fn record_result(&self, job_id: &str, result: CronJobResult) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.last_run = Some(result.executed_at);
            job.last_result = Some(result);
        }
    }

    /// Start the scheduler
    pub async fn start(&self) -> anyhow::Result<()> {
        if self
            .running
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Ok(()); // Already running
        }

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        let jobs = self.jobs.clone();
        let event_tx = self.event_tx.clone();
        let running = self.running.clone();

        running.store(true, std::sync::atomic::Ordering::SeqCst);

        tokio::spawn(async move {
            tracing::info!("Cron scheduler started");

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        Self::check_and_run_jobs(&jobs, &event_tx).await;
                    }
                    _ = &mut shutdown_rx => {
                        tracing::info!("Cron scheduler shutting down");
                        break;
                    }
                }
            }

            running.store(false, std::sync::atomic::Ordering::SeqCst);
        });

        Ok(())
    }

    /// Stop the scheduler
    pub async fn stop(&self) {
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(());
        }
    }

    /// Check if the scheduler is running
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Check all jobs and run those that are due
    async fn check_and_run_jobs(
        jobs: &Arc<RwLock<HashMap<String, CronJob>>>,
        event_tx: &mpsc::Sender<CronEvent>,
    ) {
        let now = Utc::now();
        let jobs_read = jobs.read().await;

        for job in jobs_read.values() {
            if !job.enabled {
                continue;
            }

            // Parse schedule and check if due
            let schedule = match job.schedule.parse::<Schedule>() {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Invalid cron expression for job '{}': {}", job.id, e);
                    continue;
                }
            };

            // Get the next scheduled time after last_run (or beginning of time)
            let last_run = job.last_run.unwrap_or_else(|| {
                DateTime::from_timestamp(0, 0).unwrap_or_else(Utc::now)
            });

            // Check if there's a scheduled time between last_run and now
            if let Some(next) = schedule.after(&last_run).next() {
                if next <= now {
                    // Job is due
                    let event = CronEvent {
                        job_id: job.id.clone(),
                        payload: job.payload.clone(),
                        scheduled_at: next,
                    };

                    if let Err(e) = event_tx.send(event).await {
                        tracing::error!("Failed to emit cron event for job '{}': {}", job.id, e);
                    } else {
                        tracing::debug!("Triggered cron job '{}'", job.id);
                    }
                }
            }
        }
    }

    /// Load jobs from a store
    pub async fn load_from_store(&self, store: &super::CronStore) -> anyhow::Result<()> {
        let entries = store.list()?;
        let mut jobs = self.jobs.write().await;

        for entry in entries {
            jobs.insert(entry.job.id.clone(), entry.job);
        }

        Ok(())
    }

    /// Save jobs to a store
    pub async fn save_to_store(&self, store: &mut super::CronStore) -> anyhow::Result<()> {
        let jobs = self.jobs.read().await;

        for job in jobs.values() {
            store.save(job)?;
        }

        Ok(())
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}
