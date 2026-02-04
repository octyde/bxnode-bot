//! Cron module - Scheduling and job management
//!
//! This module provides first-class cron scheduling support:
//! - Cron expression parsing and scheduling
//! - Persistent job store (JSON)
//! - Job execution lifecycle
//!
//! # Example
//!
//! ```ignore
//! use bxnode_bot::cron::{CronScheduler, CronJob};
//!
//! let mut scheduler = CronScheduler::new();
//! scheduler.add_job(CronJob {
//!     id: "daily-backup".to_string(),
//!     schedule: "0 0 * * *".to_string(),
//!     payload: serde_json::json!({"action": "backup"}),
//!     enabled: true,
//! })?;
//! scheduler.start().await;
//! ```

pub mod scheduler;
pub mod store;

#[cfg(test)]
mod tests;

pub use scheduler::{CronEvent, CronJob, CronJobResult, CronScheduler};
pub use store::{CronEntry, CronStore};
