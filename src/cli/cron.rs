//! Cron CLI command implementations

use crate::config::Config;
use crate::cron::{CronJob, CronStore};

use super::{CronAction, CronArgs};

/// Execute a cron CLI command
pub fn execute(args: CronArgs) -> anyhow::Result<()> {
    let config = Config::load_or_default(args.config.as_ref());
    let store_path = expand_path(&config.cron.store_path);

    match args.action {
        CronAction::List => list_jobs(&store_path),
        CronAction::Add {
            id,
            schedule,
            payload,
            description,
        } => add_job(&store_path, id, schedule, payload, description),
        CronAction::Remove { id } => remove_job(&store_path, &id),
        CronAction::Run { id } => run_job(&store_path, &id),
        CronAction::Runs { id, limit } => show_runs(&store_path, id.as_deref(), limit),
    }
}

/// List all cron jobs
fn list_jobs(store_path: &str) -> anyhow::Result<()> {
    let store = CronStore::open(store_path)?;
    let jobs = store.list()?;

    if jobs.is_empty() {
        println!("No cron jobs found.");
        return Ok(());
    }

    println!("{:<20} {:<25} {:<8} {}", "ID", "SCHEDULE", "ENABLED", "DESCRIPTION");
    println!("{}", "-".repeat(80));

    for entry in jobs {
        let job = &entry.job;
        let desc = job.description.as_deref().unwrap_or("-");
        let enabled = if job.enabled { "yes" } else { "no" };
        println!("{:<20} {:<25} {:<8} {}", job.id, job.schedule, enabled, desc);
    }

    Ok(())
}

/// Add a new cron job
fn add_job(
    store_path: &str,
    id: String,
    schedule: String,
    payload: String,
    description: Option<String>,
) -> anyhow::Result<()> {
    // Validate cron expression
    schedule
        .parse::<cron::Schedule>()
        .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", schedule, e))?;

    // Parse payload
    let payload: serde_json::Value = serde_json::from_str(&payload)
        .map_err(|e| anyhow::anyhow!("Invalid JSON payload: {}", e))?;

    let job = CronJob {
        id: id.clone(),
        schedule,
        payload,
        enabled: true,
        description,
        last_run: None,
        last_result: None,
    };

    let mut store = CronStore::open(store_path)?;

    // Check if job already exists
    if store.get(&id).is_some() {
        anyhow::bail!("Job '{}' already exists. Use 'cron remove' first.", id);
    }

    store.save(&job)?;
    println!("Added cron job '{}'", id);

    Ok(())
}

/// Remove a cron job
fn remove_job(store_path: &str, id: &str) -> anyhow::Result<()> {
    let mut store = CronStore::open(store_path)?;

    match store.remove(id)? {
        Some(_) => {
            println!("Removed cron job '{}'", id);
            Ok(())
        }
        None => {
            anyhow::bail!("Job '{}' not found", id);
        }
    }
}

/// Manually run a cron job
fn run_job(store_path: &str, id: &str) -> anyhow::Result<()> {
    let store = CronStore::open(store_path)?;

    match store.get(id) {
        Some(entry) => {
            println!("Triggering job '{}'...", id);
            println!("Payload: {}", serde_json::to_string_pretty(&entry.job.payload)?);
            // Note: Actual execution happens in the gateway scheduler
            // This just shows what would be triggered
            println!(
                "\nNote: Jobs are executed by the running gateway server."
            );
            println!("Use 'bxnode-bot serve' to start the scheduler.");
            Ok(())
        }
        None => {
            anyhow::bail!("Job '{}' not found", id);
        }
    }
}

/// Show recent runs for jobs
fn show_runs(store_path: &str, id: Option<&str>, limit: usize) -> anyhow::Result<()> {
    let store = CronStore::open(store_path)?;
    let all_jobs = store.list()?;

    let jobs: Vec<_> = match id {
        Some(id) => all_jobs.into_iter().filter(|e| e.job.id == id).collect(),
        None => all_jobs,
    };

    if jobs.is_empty() {
        if let Some(id) = id {
            anyhow::bail!("Job '{}' not found", id);
        } else {
            println!("No cron jobs found.");
            return Ok(());
        }
    }

    for entry in jobs.iter().take(limit) {
        let job = &entry.job;
        println!("Job: {}", job.id);

        if let Some(ref result) = job.last_result {
            let status = if result.success { "SUCCESS" } else { "FAILED" };
            println!("  Last run: {}", result.executed_at);
            println!("  Status: {} ({}ms)", status, result.duration_ms);
            if let Some(ref error) = result.error {
                println!("  Error: {}", error);
            }
        } else {
            println!("  No runs recorded.");
        }

        println!();
    }

    Ok(())
}

/// Expand ~ to home directory
fn expand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }
    path.to_string()
}
