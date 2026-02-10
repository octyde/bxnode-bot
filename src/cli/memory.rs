//! Memory CLI command implementations

use crate::config::Config;
use crate::memory::{MemoryScope, MemoryStore};

use super::{MemoryAction, MemoryArgs};

/// Execute a memory CLI command
pub fn execute(args: MemoryArgs) -> anyhow::Result<()> {
    let config = Config::load_or_default(args.config.as_ref());
    let store_path = expand_path(&config.memory.store_path);

    match args.action {
        MemoryAction::List {
            agent,
            channel,
            user,
            limit,
        } => list_memories(&store_path, agent, channel, user, limit),
        MemoryAction::Search {
            query,
            agent,
            limit,
        } => search_memories(&store_path, &query, agent, limit),
        MemoryAction::Stats => show_stats(&store_path),
        MemoryAction::Get { id } => get_memory(&store_path, &id),
        MemoryAction::Delete { id } => delete_memory(&store_path, &id),
        MemoryAction::Compact => compact_store(&store_path),
    }
}

/// List memories with optional filters
fn list_memories(
    store_path: &str,
    agent: Option<String>,
    channel: Option<String>,
    user: Option<String>,
    limit: usize,
) -> anyhow::Result<()> {
    let store = MemoryStore::open(store_path)?;

    let scope = MemoryScope {
        agent_id: agent.unwrap_or_default(),
        channel_id: channel,
        user_id: user,
        session_id: None,
    };

    let memories = store.list(&scope);

    if memories.is_empty() {
        println!("No memories found.");
        return Ok(());
    }

    println!(
        "{:<36} {:<10} {:<20} {}",
        "ID", "IMPORTANCE", "CREATED", "SUMMARY/CONTENT"
    );
    println!("{}", "-".repeat(90));

    for record in memories.iter().take(limit) {
        let created = chrono::DateTime::from_timestamp_millis(record.created_at)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let preview = record
            .summary
            .clone()
            .unwrap_or_else(|| truncate(&record.content, 40));

        println!(
            "{:<36} {:<10} {:<20} {}",
            record.id, record.importance, created, preview
        );
    }

    let total = memories.len();
    if total > limit {
        println!("\n... and {} more (use --limit to show more)", total - limit);
    }

    Ok(())
}

/// Search memories by query
fn search_memories(
    store_path: &str,
    query: &str,
    agent: Option<String>,
    limit: usize,
) -> anyhow::Result<()> {
    let store = MemoryStore::open(store_path)?;

    let scope = MemoryScope {
        agent_id: agent.unwrap_or_default(),
        channel_id: None,
        user_id: None,
        session_id: None,
    };

    let results = store.search(query, &scope, limit);

    if results.is_empty() {
        println!("No memories found for query: {}", query);
        return Ok(());
    }

    println!(
        "{:<36} {:<8} {:<20} {}",
        "ID", "SCORE", "CREATED", "SUMMARY/CONTENT"
    );
    println!("{}", "-".repeat(90));

    for result in &results {
        let created = chrono::DateTime::from_timestamp_millis(result.created_at)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let preview = result
            .summary
            .clone()
            .unwrap_or_else(|| truncate(&result.content_preview, 40));

        println!(
            "{:<36} {:<8.2} {:<20} {}",
            result.id, result.score, created, preview
        );
    }

    Ok(())
}

/// Show memory statistics
fn show_stats(store_path: &str) -> anyhow::Result<()> {
    let store = MemoryStore::open(store_path)?;
    let stats = store.stats();

    println!("Memory Store Statistics");
    println!("{}", "-".repeat(40));
    println!("Total records:     {}", stats.total_records);
    println!("Active records:    {}", stats.active_records);
    println!("Deleted records:   {}", stats.deleted_records);
    println!("Index tokens:      {}", stats.index_tokens);
    println!("Store path:        {}", store_path);

    // Show importance breakdown if there are records
    if !stats.by_importance.is_empty() {
        println!("\nBy Importance:");
        let mut importances: Vec<_> = stats.by_importance.iter().collect();
        importances.sort_by_key(|(k, _)| *k);
        for (imp, count) in importances {
            println!("  {}: {}", imp, count);
        }
    }

    Ok(())
}

/// Get a specific memory by ID
fn get_memory(store_path: &str, id: &str) -> anyhow::Result<()> {
    let store = MemoryStore::open(store_path)?;

    match store.get(id) {
        Some(record) => {
            println!("ID:          {}", record.id);
            println!("Importance:  {}", record.importance);

            let created = chrono::DateTime::from_timestamp_millis(record.created_at)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("Created:     {}", created);

            let updated = chrono::DateTime::from_timestamp_millis(record.updated_at)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("Updated:     {}", updated);

            if !record.tags.is_empty() {
                println!("Tags:        {}", record.tags.join(", "));
            }

            if let Some(ref summary) = record.summary {
                println!("Summary:     {}", summary);
            }

            println!("\nScope:");
            println!("  Agent:     {}", record.scope.agent_id);
            if let Some(ref ch) = record.scope.channel_id {
                println!("  Channel:   {}", ch);
            }
            if let Some(ref u) = record.scope.user_id {
                println!("  User:      {}", u);
            }
            if let Some(ref s) = record.scope.session_id {
                println!("  Session:   {}", s);
            }

            println!("\nContent:");
            println!("{}", record.content);

            Ok(())
        }
        None => {
            anyhow::bail!("Memory '{}' not found", id);
        }
    }
}

/// Delete a memory (soft delete)
fn delete_memory(store_path: &str, id: &str) -> anyhow::Result<()> {
    let mut store = MemoryStore::open(store_path)?;

    match store.delete(id)? {
        true => {
            println!("Deleted memory '{}'", id);
            Ok(())
        }
        false => {
            anyhow::bail!("Memory '{}' not found", id);
        }
    }
}

/// Compact the memory store
fn compact_store(store_path: &str) -> anyhow::Result<()> {
    let mut store = MemoryStore::open(store_path)?;
    let stats_before = store.stats();

    store.compact()?;

    let stats_after = store.stats();
    let removed = stats_before.total_records - stats_after.total_records;

    println!("Compacted memory store");
    println!("  Removed {} deleted record(s)", removed);
    println!("  Active records: {}", stats_after.active_records);

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

/// Truncate a string to a maximum length
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
