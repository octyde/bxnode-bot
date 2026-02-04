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
        created_at: now,
        updated_at: now,
        title: Some("My Chat".to_string()),
        metadata: json!({"key": "value"}),
    };

    assert_eq!(session.id, "sess-001");
    assert_eq!(session.channel, "telegram");
    assert_eq!(session.chat_id, "12345");
    assert!(session.title.is_some());
}

#[test]
fn test_session_serialization() {
    let session = create_test_session("sess-002");

    let json = serde_json::to_value(&session).unwrap();

    assert_eq!(json["id"], "sess-002");
    assert_eq!(json["channel"], "test");
    assert!(json["created_at"].is_string());
}

#[test]
fn test_session_deserialization() {
    let json = json!({
        "id": "sess-003",
        "channel": "discord",
        "chat_id": "guild-123",
        "agent_id": "default",
        "created_at": "2024-01-15T10:30:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "title": "Discord Chat",
        "metadata": {}
    });

    let session: Session = serde_json::from_value(json).unwrap();

    assert_eq!(session.id, "sess-003");
    assert_eq!(session.channel, "discord");
    assert_eq!(session.title, Some("Discord Chat".to_string()));
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
            output: "Sunny, 22°C".to_string(),
        },
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["type"], "tool_result");
    assert_eq!(json["tool"], "weather");
    assert_eq!(json["success"], true);
    assert_eq!(json["output"], "Sunny, 22°C");
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
    manager.create(session.clone()).await.unwrap();

    // Verify files were created
    let session_dir = temp_dir.path().join("test-session-1");
    assert!(session_dir.exists());
    assert!(session_dir.join("session.json").exists());
}

#[tokio::test]
async fn test_session_manager_load() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let session = create_test_session("load-test");
    manager.create(session.clone()).await.unwrap();

    let loaded = manager.load("load-test").await.unwrap();
    assert!(loaded.is_some());

    let loaded_session = loaded.unwrap();
    assert_eq!(loaded_session.id, "load-test");
    assert_eq!(loaded_session.channel, "test");
}

#[tokio::test]
async fn test_session_manager_load_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let loaded = manager.load("nonexistent").await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_session_manager_append() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    let session = create_test_session("append-test");
    manager.create(session).await.unwrap();

    // Append a user message
    let entry1 = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::User {
            user_id: "user-1".to_string(),
            content: "Hello".to_string(),
        },
    };
    manager.append("append-test", entry1).await.unwrap();

    // Append an assistant message
    let entry2 = TranscriptEntry {
        timestamp: Utc::now(),
        entry_type: TranscriptEntryType::Assistant {
            content: "Hi!".to_string(),
            model: "test-model".to_string(),
        },
    };
    manager.append("append-test", entry2).await.unwrap();

    // Verify transcript file exists and has content
    let transcript_path = temp_dir.path().join("append-test").join("transcript.jsonl");
    assert!(transcript_path.exists());

    let content = std::fs::read_to_string(&transcript_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);

    // Verify first line is user message
    let first: TranscriptEntry = serde_json::from_str(lines[0]).unwrap();
    match first.entry_type {
        TranscriptEntryType::User { user_id, .. } => assert_eq!(user_id, "user-1"),
        _ => panic!("Expected User entry"),
    }
}

#[tokio::test]
async fn test_session_manager_list() {
    let temp_dir = TempDir::new().unwrap();
    let manager = SessionManager::new(temp_dir.path().to_path_buf());

    // Create multiple sessions
    manager.create(create_test_session("list-1")).await.unwrap();
    manager.create(create_test_session("list-2")).await.unwrap();
    manager.create(create_test_session("list-3")).await.unwrap();

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

    let sessions = manager.list().await.unwrap();
    assert!(sessions.is_empty());
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
