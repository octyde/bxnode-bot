//! Unit tests for the session module

use super::*;
use chrono::Utc;
use serde_json::json;
use tempfile::TempDir;

fn create_test_session(id: &str) -> Session {
    let now = Utc::now();
    Session {
        id: id.to_string(),
        channel: "test".to_string(),
        chat_id: "chat-123".to_string(),
        agent_id: "default".to_string(),
        project: "default".to_string(),
        created_at: now,
        updated_at: now,
        title: Some("Test Session".to_string()),
        metadata: json!({}),
    }
}

#[test]
fn test_session_creation() {
    let now = Utc::now();
    let session = Session {
        id: "sess-001".to_string(),
        channel: "telegram".to_string(),
        chat_id: "12345".to_string(),
        agent_id: "agent-1".to_string(),
        project: "default".to_string(),
        created_at: now,
        updated_at: now,
        title: Some("My Chat".to_string()),
        metadata: json!({"key": "value"}),
    };

    assert_eq!(session.id, "sess-001");
    assert_eq!(session.channel, "telegram");
    assert_eq!(session.chat_id, "12345");
    assert_eq!(session.project, "default");
    assert!(session.title.is_some());
}

#[test]
fn test_session_serialization() {
    let session = create_test_session("sess-002");

    let json = serde_json::to_value(&session).unwrap();

    assert_eq!(json["id"], "sess-002");
    assert_eq!(json["channel"], "test");
    assert_eq!(json["project"], "default");
    assert!(json["created_at"].is_string());
}

#[test]
fn test_session_deserialization() {
    let json = json!({
        "id": "sess-003",
        "channel": "discord",
        "chat_id": "guild-123",
        "agent_id": "default",
        "project": "my-project",
        "created_at": "2024-01-15T10:30:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "title": "Discord Chat",
        "metadata": {}
    });

    let session: Session = serde_json::from_value(json).unwrap();

    assert_eq!(session.id, "sess-003");
    assert_eq!(session.channel, "discord");
    assert_eq!(session.project, "my-project");
    assert_eq!(session.title, Some("Discord Chat".to_string()));
}

#[test]
fn test_session_deserialization_default_project() {
    // Old sessions without project field should default to "default"
    let json = json!({
        "id": "sess-old",
        "channel": "telegram",
        "chat_id": "123",
        "agent_id": "default",
        "created_at": "2024-01-15T10:30:00Z",
        "updated_at": "2024-01-15T10:30:00Z"
    });

    let session: Session = serde_json::from_value(json).unwrap();
    assert_eq!(session.project, "default");
}

#[test]
fn test_transcript_entry_user() {
    let entry = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::User {
            user_id: "user-1".to_string(),
            content: "Hello!".to_string(),
        },
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["type"], "user");
    assert_eq!(json["user_id"], "user-1");
    assert_eq!(json["content"], "Hello!");
}

#[test]
fn test_transcript_entry_assistant() {
    let entry = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::Assistant {
            content: "Hi there!".to_string(),
            model: "claude-3-opus".to_string(),
        },
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["type"], "assistant");
    assert_eq!(json["content"], "Hi there!");
    assert_eq!(json["model"], "claude-3-opus");
}

#[test]
fn test_transcript_entry_tool_use() {
    let entry = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::ToolUse {
            tool: "weather".to_string(),
            input: json!({"location": "London"}),
        },
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["type"], "tool_use");
    assert_eq!(json["tool"], "weather");
    assert_eq!(json["input"]["location"], "London");
}

#[test]
fn test_transcript_entry_tool_result() {
    let entry = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::ToolResult {
            tool: "weather".to_string(),
            success: true,
            output: "Sunny, 22\u{00b0}C".to_string(),
        },
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["type"], "tool_result");
    assert_eq!(json["tool"], "weather");
    assert_eq!(json["success"], true);
    assert_eq!(json["output"], "Sunny, 22\u{00b0}C");
}

#[test]
fn test_transcript_entry_system() {
    let entry = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::System {
            message: "Session started".to_string(),
        },
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["type"], "system");
    assert_eq!(json["message"], "Session started");
}

#[test]
fn test_session_manager_new() {
    let temp_dir = TempDir::new().unwrap();
    let _manager = SessionManager::new(temp_dir.path().to_path_buf());

    // Just verify it creates without error
    assert!(temp_dir.path().exists());
}

#[tokio::test]
async fn test_session_manager_create() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let session = create_test_session("test-session-1");
    manager.create(&session).await.unwrap();

    // Verify files were created under project directory
    let session_dir = temp_dir.path().join("default").join("test-session-1");
    assert!(session_dir.exists());
    assert!(session_dir.join("session.json").exists());
}

#[tokio::test]
async fn test_session_manager_load() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let session = create_test_session("load-test");
    manager.create(&session).await.unwrap();

    let loaded = manager.load("load-test").await.unwrap();
    assert!(loaded.is_some());

    let loaded_session = loaded.unwrap();
    assert_eq!(loaded_session.id, "load-test");
    assert_eq!(loaded_session.channel, "test");
    assert_eq!(loaded_session.project, "default");
}

#[tokio::test]
async fn test_session_manager_load_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());
    manager.ensure_base_dir().await.unwrap();

    let loaded = manager.load("nonexistent").await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_session_manager_update() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let mut session = create_test_session("update-test");
    manager.create(&session).await.unwrap();

    session.title = Some("Updated Title".to_string());
    manager.update(&session).await.unwrap();

    let loaded = manager.load("update-test").await.unwrap().unwrap();
    assert_eq!(loaded.title, Some("Updated Title".to_string()));
}

#[tokio::test]
async fn test_session_manager_delete() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let session = create_test_session("delete-test");
    manager.create(&session).await.unwrap();

    manager.delete("default", "delete-test").await.unwrap();
    let loaded = manager.load("delete-test").await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_session_manager_append_and_load_transcript() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let session = create_test_session("transcript-test");
    manager.create(&session).await.unwrap();

    // Append a user message
    let entry1 = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::User {
            user_id: "user-1".to_string(),
            content: "Hello".to_string(),
        },
    };
    manager.append("default", "transcript-test", &entry1).await.unwrap();

    // Append an assistant message
    let entry2 = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::Assistant {
            content: "Hi!".to_string(),
            model: "test-model".to_string(),
        },
    };
    manager.append("default", "transcript-test", &entry2).await.unwrap();

    // Load transcript
    let entries = manager.load_transcript("default", "transcript-test").await.unwrap();
    assert_eq!(entries.len(), 2);

    match &entries[0].entry_type {
        TranscriptEntryType::User { user_id, .. } => assert_eq!(user_id, "user-1"),
        _ => panic!("Expected User entry"),
    }
    match &entries[1].entry_type {
        TranscriptEntryType::Assistant { content, .. } => assert_eq!(content, "Hi!"),
        _ => panic!("Expected Assistant entry"),
    }
}

#[tokio::test]
async fn test_session_manager_list() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    // Create multiple sessions
    manager.create(&create_test_session("list-1")).await.unwrap();
    manager.create(&create_test_session("list-2")).await.unwrap();
    manager.create(&create_test_session("list-3")).await.unwrap();

    let sessions = manager.list().await.unwrap();
    assert_eq!(sessions.len(), 3);

    let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"list-1"));
    assert!(ids.contains(&"list-2"));
    assert!(ids.contains(&"list-3"));
}

#[tokio::test]
async fn test_session_manager_list_empty() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());
    manager.ensure_base_dir().await.unwrap();

    let sessions = manager.list().await.unwrap();
    assert!(sessions.is_empty());
}

#[tokio::test]
async fn test_session_manager_list_for_chat() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let mut s1 = create_test_session("chat-1");
    s1.channel = "telegram".to_string();
    s1.chat_id = "111".to_string();
    manager.create(&s1).await.unwrap();

    let mut s2 = create_test_session("chat-2");
    s2.channel = "telegram".to_string();
    s2.chat_id = "111".to_string();
    manager.create(&s2).await.unwrap();

    let mut s3 = create_test_session("chat-3");
    s3.channel = "discord".to_string();
    s3.chat_id = "222".to_string();
    manager.create(&s3).await.unwrap();

    let telegram_sessions = manager.list_for_chat("telegram", "111").await.unwrap();
    assert_eq!(telegram_sessions.len(), 2);

    let discord_sessions = manager.list_for_chat("discord", "222").await.unwrap();
    assert_eq!(discord_sessions.len(), 1);
}

#[tokio::test]
async fn test_projects() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());
    manager.ensure_base_dir().await.unwrap();

    // Default project should exist
    let projects = manager.list_projects().await.unwrap();
    assert!(projects.contains(&"default".to_string()));

    // Create a new project
    manager.create_project("my-project").await.unwrap();
    let projects = manager.list_projects().await.unwrap();
    assert_eq!(projects[0], "default"); // default always first
    assert!(projects.contains(&"my-project".to_string()));

    // Create session in new project
    let mut session = create_test_session("proj-session");
    session.project = "my-project".to_string();
    manager.create(&session).await.unwrap();

    let count = manager.count_sessions_in_project("my-project").await.unwrap();
    assert_eq!(count, 1);

    // Delete project
    manager.delete_project("my-project").await.unwrap();
    let projects = manager.list_projects().await.unwrap();
    assert!(!projects.contains(&"my-project".to_string()));
}

#[tokio::test]
async fn test_delete_default_project_fails() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());
    manager.ensure_base_dir().await.unwrap();

    let result = manager.delete_project("default").await;
    assert!(result.is_err());
}

#[test]
fn test_active_session_map() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("active_sessions.json");

    let mut map = ActiveSessionMap::load(path.clone());

    // Initially empty
    assert!(map.get("telegram:123").is_none());

    // Set and get
    map.set("telegram:123".to_string(), ActiveSessionInfo {
        session_id: "sess-1".to_string(),
        project: "default".to_string(),
    });
    let info = map.get("telegram:123").unwrap();
    assert_eq!(info.session_id, "sess-1");
    assert_eq!(info.project, "default");

    // Verify persistence
    let map2 = ActiveSessionMap::load(path);
    let info2 = map2.get("telegram:123").unwrap();
    assert_eq!(info2.session_id, "sess-1");

    // Remove
    let mut map2 = map2;
    map2.remove("telegram:123");
    assert!(map2.get("telegram:123").is_none());
}

#[test]
fn test_session_without_title() {
    let json = json!({
        "id": "no-title",
        "channel": "web",
        "chat_id": "abc",
        "agent_id": "default",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:00:00Z"
    });

    let session: Session = serde_json::from_value(json).unwrap();

    assert!(session.title.is_none());
    assert!(session.metadata.is_null());
    assert_eq!(session.project, "default");
}

#[test]
fn test_transcript_entry_serialization_roundtrip() {
    let entries = vec![
        TranscriptEntry {
            timestamp: Utc::now(),
            entry_type: TranscriptEntryType::User {
                user_id: "u1".to_string(),
                content: "Test".to_string(),
            },
        },
        TranscriptEntry {
            timestamp: Utc::now(),
            entry_type: TranscriptEntryType::Assistant {
                content: "Response".to_string(),
                model: "test".to_string(),
            },
        },
        TranscriptEntry {
            timestamp: Utc::now(),
            entry_type: TranscriptEntryType::ToolUse {
                tool: "calc".to_string(),
                input: json!({"expr": "1+1"}),
            },
        },
        TranscriptEntry {
            timestamp: Utc::now(),
            entry_type: TranscriptEntryType::ToolResult {
                tool: "calc".to_string(),
                success: true,
                output: "2".to_string(),
            },
        },
        TranscriptEntry {
            timestamp: Utc::now(),
            entry_type: TranscriptEntryType::System {
                message: "Done".to_string(),
            },
        },
    ];

    for entry in entries {
        let json = serde_json::to_string(&entry).unwrap();
        let restored: TranscriptEntry = serde_json::from_str(&json).unwrap();

        // Verify type matches
        match (&entry.entry_type, &restored.entry_type) {
            (TranscriptEntryType::User { .. }, TranscriptEntryType::User { .. }) => {}
            (TranscriptEntryType::Assistant { .. }, TranscriptEntryType::Assistant { .. }) => {}
            (TranscriptEntryType::ToolUse { .. }, TranscriptEntryType::ToolUse { .. }) => {}
            (TranscriptEntryType::ToolResult { .. }, TranscriptEntryType::ToolResult { .. }) => {}
            (TranscriptEntryType::System { .. }, TranscriptEntryType::System { .. }) => {}
            _ => panic!("Entry type mismatch after roundtrip"),
        }
    }
}

#[test]
fn test_transcript_entry_deserialization_user() {
    let json = json!({
        "timestamp": "2024-01-15T12:00:00Z",
        "type": "user",
        "user_id": "test-user",
        "content": "What's the weather?"
    });

    let entry: TranscriptEntry = serde_json::from_value(json).unwrap();

    match entry.entry_type {
        TranscriptEntryType::User { user_id, content } => {
            assert_eq!(user_id, "test-user");
            assert_eq!(content, "What's the weather?");
        }
        _ => panic!("Expected User entry type"),
    }
}

#[test]
fn test_transcript_entry_deserialization_assistant() {
    let json = json!({
        "timestamp": "2024-01-15T12:01:00Z",
        "type": "assistant",
        "content": "The weather is sunny.",
        "model": "gpt-4"
    });

    let entry: TranscriptEntry = serde_json::from_value(json).unwrap();

    match entry.entry_type {
        TranscriptEntryType::Assistant { content, model } => {
            assert_eq!(content, "The weather is sunny.");
            assert_eq!(model, "gpt-4");
        }
        _ => panic!("Expected Assistant entry type"),
    }
}
