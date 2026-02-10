//! Tests for the memory module

use super::*;
use tempfile::tempdir;

#[test]
fn test_memory_record_creation() {
    let scope = MemoryScope::agent("test-agent");
    let record = MemoryRecord::new(scope, "Test content", Some("Test summary".to_string()));

    assert!(!record.id.is_empty());
    assert_eq!(record.content, "Test content");
    assert_eq!(record.summary, Some("Test summary".to_string()));
    assert_eq!(record.importance, 5); // Default
    assert!(!record.is_deleted());
}

#[test]
fn test_memory_record_builder() {
    let scope = MemoryScope::user("agent", "channel", "user");
    let record = MemoryRecord::builder()
        .scope(scope.clone())
        .content("Built content")
        .summary("Built summary")
        .tags(vec!["tag1".to_string(), "tag2".to_string()])
        .importance(8)
        .ttl_days(30)
        .provenance("test-source")
        .build()
        .unwrap();

    assert_eq!(record.content, "Built content");
    assert_eq!(record.summary, Some("Built summary".to_string()));
    assert_eq!(record.tags, vec!["tag1", "tag2"]);
    assert_eq!(record.importance, 8);
    assert_eq!(record.ttl_days, Some(30));
    assert_eq!(record.provenance, Some("test-source".to_string()));
}

#[test]
fn test_memory_record_builder_missing_fields() {
    // Missing scope
    let result = MemoryRecord::builder().content("content").build();
    assert!(result.is_err());

    // Missing content
    let result = MemoryRecord::builder()
        .scope(MemoryScope::agent("agent"))
        .build();
    assert!(result.is_err());
}

#[test]
fn test_memory_record_content_preview() {
    let scope = MemoryScope::agent("test");
    let long_content = "A".repeat(500);
    let record = MemoryRecord::new(scope, &long_content, None);

    let preview = record.content_preview(100);
    assert_eq!(preview.len(), 103); // 100 + "..."
    assert!(preview.ends_with("..."));

    let short_record = MemoryRecord::new(MemoryScope::agent("test"), "Short", None);
    let preview = short_record.content_preview(100);
    assert_eq!(preview, "Short");
}

#[test]
fn test_memory_record_expiration() {
    let scope = MemoryScope::agent("test");
    let mut record = MemoryRecord::new(scope, "Content", None);
    record.ttl_days = Some(1);

    let now = chrono::Utc::now().timestamp_millis();

    // Just created, not expired
    assert!(!record.is_expired(now));

    // After TTL, should be expired
    let future = now + 2 * 24 * 60 * 60 * 1000; // 2 days later
    assert!(record.is_expired(future));
}

#[test]
fn test_memory_scope_agent() {
    let scope = MemoryScope::agent("my-agent");
    assert_eq!(scope.agent_id, "my-agent");
    assert!(scope.channel_id.is_none());
    assert!(scope.user_id.is_none());
}

#[test]
fn test_memory_scope_channel() {
    let scope = MemoryScope::channel("agent", "telegram");
    assert_eq!(scope.agent_id, "agent");
    assert_eq!(scope.channel_id, Some("telegram".to_string()));
    assert!(scope.user_id.is_none());
}

#[test]
fn test_memory_scope_user() {
    let scope = MemoryScope::user("agent", "discord", "user123");
    assert_eq!(scope.agent_id, "agent");
    assert_eq!(scope.channel_id, Some("discord".to_string()));
    assert_eq!(scope.user_id, Some("user123".to_string()));
}

#[test]
fn test_memory_scope_matches() {
    let record_scope = MemoryScope::user("agent", "channel", "user1");

    // Exact match
    let query = MemoryScope::user("agent", "channel", "user1");
    assert!(record_scope.matches(&query));

    // Agent-only query matches all in that agent
    let query = MemoryScope::agent("agent");
    assert!(record_scope.matches(&query));

    // Channel query matches all in that channel
    let query = MemoryScope::channel("agent", "channel");
    assert!(record_scope.matches(&query));

    // Different agent doesn't match
    let query = MemoryScope::agent("other-agent");
    assert!(!record_scope.matches(&query));

    // Different user doesn't match
    let query = MemoryScope::user("agent", "channel", "user2");
    assert!(!record_scope.matches(&query));
}

#[test]
fn test_memory_store_in_memory() {
    let mut store = MemoryStore::in_memory();
    assert!(store.is_empty());

    let scope = MemoryScope::agent("test");
    let record = MemoryRecord::new(scope.clone(), "Test memory", None);
    let id = store.store(record).unwrap();

    assert_eq!(store.len(), 1);
    assert!(!store.is_empty());

    let retrieved = store.get(&id).unwrap();
    assert_eq!(retrieved.content, "Test memory");
}

#[test]
fn test_memory_store_persistence() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("memory.jsonl");

    let scope = MemoryScope::agent("test");
    let id;

    // Create and store
    {
        let mut store = MemoryStore::open(&path).unwrap();
        let record = MemoryRecord::new(scope.clone(), "Persistent memory", None);
        id = store.store(record).unwrap();
        assert_eq!(store.len(), 1);
    }

    // Reload and verify
    {
        let store = MemoryStore::open(&path).unwrap();
        assert_eq!(store.len(), 1);
        let retrieved = store.get(&id).unwrap();
        assert_eq!(retrieved.content, "Persistent memory");
    }
}

#[test]
fn test_memory_store_soft_delete() {
    let mut store = MemoryStore::in_memory();

    let scope = MemoryScope::agent("test");
    let record = MemoryRecord::new(scope.clone(), "To be deleted", None);
    let id = store.store(record).unwrap();

    assert_eq!(store.len(), 1);

    // Delete
    let deleted = store.delete(&id).unwrap();
    assert!(deleted);

    // Should no longer be accessible
    assert_eq!(store.len(), 0);
    assert!(store.get(&id).is_none());

    // Delete again should return false
    let deleted = store.delete(&id).unwrap();
    assert!(!deleted);
}

#[test]
fn test_memory_store_soft_delete_persistence() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("memory.jsonl");

    let scope = MemoryScope::agent("test");
    let id;

    // Create, store, and delete
    {
        let mut store = MemoryStore::open(&path).unwrap();
        let record = MemoryRecord::new(scope.clone(), "To be deleted", None);
        id = store.store(record).unwrap();
        store.delete(&id).unwrap();
        assert_eq!(store.len(), 0);
    }

    // Reload and verify still deleted
    {
        let store = MemoryStore::open(&path).unwrap();
        assert_eq!(store.len(), 0);
        assert!(store.get(&id).is_none());
    }
}

#[test]
fn test_memory_store_list() {
    let mut store = MemoryStore::in_memory();

    let scope1 = MemoryScope::user("agent", "channel", "user1");
    let scope2 = MemoryScope::user("agent", "channel", "user2");

    store
        .store(MemoryRecord::new(scope1.clone(), "User1 memory 1", None))
        .unwrap();
    store
        .store(MemoryRecord::new(scope1.clone(), "User1 memory 2", None))
        .unwrap();
    store
        .store(MemoryRecord::new(scope2.clone(), "User2 memory", None))
        .unwrap();

    // User1 should see 2 memories
    let list = store.list(&scope1);
    assert_eq!(list.len(), 2);

    // User2 should see 1 memory
    let list = store.list(&scope2);
    assert_eq!(list.len(), 1);

    // Agent scope should see all 3
    let agent_scope = MemoryScope::agent("agent");
    let list = store.list(&agent_scope);
    assert_eq!(list.len(), 3);
}

#[test]
fn test_memory_store_search() {
    let mut store = MemoryStore::in_memory();

    let scope = MemoryScope::agent("test");

    store
        .store(MemoryRecord::new(
            scope.clone(),
            "The quick brown fox jumps over the lazy dog",
            None,
        ))
        .unwrap();
    store
        .store(MemoryRecord::new(scope.clone(), "A lazy cat sleeps all day", None))
        .unwrap();
    store
        .store(MemoryRecord::new(scope.clone(), "Python is a programming language", None))
        .unwrap();

    // Search for "lazy"
    let results = store.search("lazy", &scope, 5);
    assert_eq!(results.len(), 2);

    // Search for "fox"
    let results = store.search("fox", &scope, 5);
    assert_eq!(results.len(), 1);
    assert!(results[0].content_preview.contains("fox"));

    // Search for "programming"
    let results = store.search("programming", &scope, 5);
    assert_eq!(results.len(), 1);
    assert!(results[0].content_preview.contains("Python"));
}

#[test]
fn test_memory_store_search_ranking() {
    let mut store = MemoryStore::in_memory();

    let scope = MemoryScope::agent("test");

    // Add records with different importance
    let mut high_importance = MemoryRecord::new(scope.clone(), "Important memory about tests", None);
    high_importance.importance = 10;

    let mut low_importance = MemoryRecord::new(scope.clone(), "Less important memory about tests", None);
    low_importance.importance = 1;

    store.store(low_importance).unwrap();
    store.store(high_importance).unwrap();

    // Search should rank by score (importance affects score)
    let results = store.search("tests", &scope, 5);
    assert_eq!(results.len(), 2);
    assert!(results[0].importance >= results[1].importance);
}

#[test]
fn test_memory_store_search_with_importance_filter() {
    let mut store = MemoryStore::in_memory();

    let scope = MemoryScope::agent("test");

    let mut high = MemoryRecord::new(scope.clone(), "High importance memory", None);
    high.importance = 8;

    let mut low = MemoryRecord::new(scope.clone(), "Low importance memory", None);
    low.importance = 2;

    store.store(high).unwrap();
    store.store(low).unwrap();

    // Filter to only high importance
    let results = store.search_with_importance("memory", &scope, 5, 5);
    assert_eq!(results.len(), 1);
    assert!(results[0].importance >= 5);
}

#[test]
fn test_memory_store_compact() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("memory.jsonl");

    let scope = MemoryScope::agent("test");

    // Create records and delete one
    {
        let mut store = MemoryStore::open(&path).unwrap();

        let record1 = MemoryRecord::new(scope.clone(), "Keep this", None);
        let id1 = store.store(record1).unwrap();

        let record2 = MemoryRecord::new(scope.clone(), "Delete this", None);
        let id2 = store.store(record2).unwrap();

        store.delete(&id2).unwrap();

        assert_eq!(store.len(), 1);

        // Compact
        let removed = store.compact().unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.len(), 1);

        // Verify the kept record is still searchable
        let results = store.search("keep", &scope, 5);
        assert_eq!(results.len(), 1);
    }

    // Reload and verify compaction persisted
    {
        let store = MemoryStore::open(&path).unwrap();
        assert_eq!(store.len(), 1);

        let results = store.search("keep", &scope, 5);
        assert_eq!(results.len(), 1);

        // Deleted record should not appear
        let results = store.search("delete", &scope, 5);
        assert!(results.is_empty());
    }
}

#[test]
fn test_memory_store_stats() {
    let mut store = MemoryStore::in_memory();

    let scope = MemoryScope::agent("test");

    let mut r1 = MemoryRecord::new(scope.clone(), "Memory 1", None);
    r1.importance = 5;

    let mut r2 = MemoryRecord::new(scope.clone(), "Memory 2", None);
    r2.importance = 5;

    let mut r3 = MemoryRecord::new(scope.clone(), "Memory 3", None);
    r3.importance = 10;

    let id1 = store.store(r1).unwrap();
    store.store(r2).unwrap();
    store.store(r3).unwrap();
    store.delete(&id1).unwrap();

    let stats = store.stats();
    assert_eq!(stats.total_records, 3);
    assert_eq!(stats.active_records, 2);
    assert_eq!(stats.deleted_records, 1);
    assert_eq!(stats.by_importance.get(&5), Some(&1)); // One active with importance 5
    assert_eq!(stats.by_importance.get(&10), Some(&1)); // One active with importance 10
}

#[test]
fn test_memory_record_serialization() {
    let scope = MemoryScope::user("agent", "channel", "user");
    let record = MemoryRecord::builder()
        .scope(scope)
        .content("Test content")
        .summary("Test summary")
        .tags(vec!["tag1".to_string()])
        .importance(7)
        .build()
        .unwrap();

    let json = serde_json::to_string(&record).unwrap();
    let deserialized: MemoryRecord = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, record.id);
    assert_eq!(deserialized.content, record.content);
    assert_eq!(deserialized.summary, record.summary);
    assert_eq!(deserialized.tags, record.tags);
    assert_eq!(deserialized.importance, record.importance);
}
