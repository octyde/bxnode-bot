//! Tests for the cron module

use super::*;
use tempfile::tempdir;

#[test]
fn test_cron_job_creation() {
    let job = CronJob {
        id: "test-job".to_string(),
        schedule: "0 * * * *".to_string(), // Every hour
        payload: serde_json::json!({"action": "test"}),
        enabled: true,
        description: Some("Test job".to_string()),
        last_run: None,
        last_result: None,
    };

    assert_eq!(job.id, "test-job");
    assert!(job.enabled);
}

#[test]
fn test_cron_job_serialization() {
    let job = CronJob {
        id: "test-job".to_string(),
        schedule: "0 0 * * *".to_string(),
        payload: serde_json::json!({"key": "value"}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    let json = serde_json::to_string(&job).unwrap();
    let deserialized: CronJob = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, job.id);
    assert_eq!(deserialized.schedule, job.schedule);
}

#[tokio::test]
async fn test_scheduler_add_job() {
    let scheduler = CronScheduler::new();

    let job = CronJob {
        id: "test".to_string(),
        schedule: "0 0 * * * *".to_string(), // 6-field format with seconds
        payload: serde_json::json!({}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    scheduler.add_job(job.clone()).await.unwrap();

    let retrieved = scheduler.get_job("test").await;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, "test");
}

#[tokio::test]
async fn test_scheduler_remove_job() {
    let scheduler = CronScheduler::new();

    let job = CronJob {
        id: "test".to_string(),
        schedule: "0 0 * * * *".to_string(), // 6-field format with seconds
        payload: serde_json::json!({}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    scheduler.add_job(job).await.unwrap();
    assert!(scheduler.get_job("test").await.is_some());

    let removed = scheduler.remove_job("test").await;
    assert!(removed.is_some());
    assert!(scheduler.get_job("test").await.is_none());
}

#[tokio::test]
async fn test_scheduler_list_jobs() {
    let scheduler = CronScheduler::new();

    for i in 0..3 {
        let job = CronJob {
            id: format!("job-{}", i),
            schedule: "0 0 * * * *".to_string(), // 6-field format with seconds
            payload: serde_json::json!({}),
            enabled: true,
            description: None,
            last_run: None,
            last_result: None,
        };
        scheduler.add_job(job).await.unwrap();
    }

    let jobs = scheduler.list_jobs().await;
    assert_eq!(jobs.len(), 3);
}

#[tokio::test]
async fn test_scheduler_invalid_cron() {
    let scheduler = CronScheduler::new();

    let job = CronJob {
        id: "test".to_string(),
        schedule: "invalid cron".to_string(),
        payload: serde_json::json!({}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    let result = scheduler.add_job(job).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_scheduler_run_job() {
    let scheduler = CronScheduler::new();

    let job = CronJob {
        id: "test".to_string(),
        schedule: "0 0 * * * *".to_string(), // 6-field format with seconds
        payload: serde_json::json!({"action": "test"}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    scheduler.add_job(job).await.unwrap();

    let mut rx = scheduler.take_event_receiver().await.unwrap();

    scheduler.run_job("test").await.unwrap();

    let event = rx.recv().await.unwrap();
    assert_eq!(event.job_id, "test");
    assert_eq!(event.payload["action"], "test");
}

#[tokio::test]
async fn test_scheduler_run_nonexistent_job() {
    let scheduler = CronScheduler::new();

    let result = scheduler.run_job("nonexistent").await;
    assert!(result.is_err());
}

#[test]
fn test_store_create() {
    let store = CronStore::in_memory();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn test_store_save_and_get() {
    let mut store = CronStore::in_memory();

    let job = CronJob {
        id: "test".to_string(),
        schedule: "0 * * * *".to_string(),
        payload: serde_json::json!({}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    store.save(&job).unwrap();

    let entry = store.get("test");
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().job.id, "test");
}

#[test]
fn test_store_remove() {
    let mut store = CronStore::in_memory();

    let job = CronJob {
        id: "test".to_string(),
        schedule: "0 * * * *".to_string(),
        payload: serde_json::json!({}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    store.save(&job).unwrap();
    assert!(!store.is_empty());

    let removed = store.remove("test").unwrap();
    assert!(removed.is_some());
    assert!(store.is_empty());
}

#[test]
fn test_store_list() {
    let mut store = CronStore::in_memory();

    for i in 0..3 {
        let job = CronJob {
            id: format!("job-{}", i),
            schedule: "0 * * * *".to_string(),
            payload: serde_json::json!({}),
            enabled: true,
            description: None,
            last_run: None,
            last_result: None,
        };
        store.save(&job).unwrap();
    }

    let list = store.list().unwrap();
    assert_eq!(list.len(), 3);
}

#[test]
fn test_store_persistence() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cron.json");

    // Create and save
    {
        let mut store = CronStore::open(&path).unwrap();

        let job = CronJob {
            id: "persistent".to_string(),
            schedule: "0 * * * *".to_string(),
            payload: serde_json::json!({"saved": true}),
            enabled: true,
            description: None,
            last_run: None,
            last_result: None,
        };

        store.save(&job).unwrap();
    }

    // Reload and verify
    {
        let store = CronStore::open(&path).unwrap();
        let entry = store.get("persistent");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().job.payload["saved"], true);
    }
}

#[test]
fn test_store_update() {
    let mut store = CronStore::in_memory();

    let job = CronJob {
        id: "test".to_string(),
        schedule: "0 * * * *".to_string(),
        payload: serde_json::json!({"version": 1}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    store.save(&job).unwrap();

    // Update the job
    let updated = CronJob {
        id: "test".to_string(),
        schedule: "0 0 * * *".to_string(), // Changed schedule
        payload: serde_json::json!({"version": 2}),
        enabled: false,
        description: Some("Updated".to_string()),
        last_run: None,
        last_result: None,
    };

    store.save(&updated).unwrap();

    let entry = store.get("test").unwrap();
    assert_eq!(entry.job.schedule, "0 0 * * *");
    assert_eq!(entry.job.payload["version"], 2);
    assert!(!entry.job.enabled);
}

#[test]
fn test_cron_entry_metadata() {
    let job = CronJob {
        id: "test".to_string(),
        schedule: "0 * * * *".to_string(),
        payload: serde_json::json!({}),
        enabled: true,
        description: None,
        last_run: None,
        last_result: None,
    };

    let entry = CronEntry::new(job);

    assert!(entry.created_at <= chrono::Utc::now());
    assert!(entry.updated_at <= chrono::Utc::now());
    assert_eq!(entry.created_at, entry.updated_at);
}

#[test]
fn test_cron_job_result() {
    let result = CronJobResult {
        success: true,
        executed_at: chrono::Utc::now(),
        duration_ms: 150,
        error: None,
    };

    assert!(result.success);
    assert!(result.error.is_none());

    let failed = CronJobResult {
        success: false,
        executed_at: chrono::Utc::now(),
        duration_ms: 50,
        error: Some("Connection timeout".to_string()),
    };

    assert!(!failed.success);
    assert!(failed.error.is_some());
}
