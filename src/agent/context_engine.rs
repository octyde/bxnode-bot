//! Pluggable context engine — abstracts context management for the agent loop.
//!
//! The context engine controls how messages are ingested, assembled for the LLM,
//! compacted when context limits are reached, and processed after each turn.
//! The default `LegacyContextEngine` wraps the existing `truncate_to_fit()` behavior.

use async_trait::async_trait;

use super::AgentContext;
use crate::providers::Message;

/// Information about the current turn, passed to `assemble()`.
#[derive(Debug, Clone)]
pub struct TurnInfo {
    /// Model being used for the current request
    pub model: String,
    /// The user's prompt that triggered this turn
    pub user_prompt: String,
    /// Turn number within the current execution (0-indexed)
    pub turn_number: usize,
}

/// Result of context compaction.
#[derive(Debug, Clone, Default)]
pub struct CompactionResult {
    /// Number of messages removed
    pub messages_removed: usize,
    /// Optional summary of removed messages (for future LLM-based summarization)
    pub summary: Option<String>,
}

/// Trait for pluggable context engines.
///
/// Implementors control the full context lifecycle:
/// - **ingest**: called when new content enters the context
/// - **assemble**: builds the message list for the LLM call
/// - **compact**: reduces context to fit token limits
/// - **after_turn**: post-processing after a complete assistant response
#[async_trait]
pub trait ContextEngine: Send + Sync {
    /// Called when new content (user message, tool result) enters the context.
    /// Default: no-op (messages are added directly to AgentContext).
    async fn ingest(&self, _context: &mut AgentContext, _message: &Message) {}

    /// Assemble messages for the provider call.
    /// Can reorder, inject RAG context, filter, etc.
    async fn assemble(&self, context: &AgentContext, turn: &TurnInfo) -> Vec<Message>;

    /// Compact the context to fit within token limits.
    async fn compact(&self, context: &mut AgentContext) -> CompactionResult;

    /// Called after a complete turn (user message + assistant response).
    /// Can be used for summary generation, memory updates, etc.
    async fn after_turn(&self, _context: &mut AgentContext, _assistant_response: &str) {}
}

/// Default context engine wrapping existing truncate_to_fit() behavior.
pub struct LegacyContextEngine;

#[async_trait]
impl ContextEngine for LegacyContextEngine {
    async fn assemble(&self, context: &AgentContext, _turn: &TurnInfo) -> Vec<Message> {
        context.messages_for_provider()
    }

    async fn compact(&self, context: &mut AgentContext) -> CompactionResult {
        let removed = context.truncate_to_fit();
        CompactionResult {
            messages_removed: removed,
            summary: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::ContextConfig;
    use crate::providers::Role;

    #[tokio::test]
    async fn test_legacy_engine_assemble() {
        let engine = LegacyContextEngine;
        let mut ctx = AgentContext::new(ContextConfig::default());
        ctx.set_system_prompt("You are helpful.".to_string());
        ctx.add_user_message("Hello");

        let turn = TurnInfo {
            model: "test-model".to_string(),
            user_prompt: "Hello".to_string(),
            turn_number: 0,
        };

        let messages = engine.assemble(&ctx, &turn).await;
        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[1].content, "Hello");
    }

    #[tokio::test]
    async fn test_legacy_engine_compact_no_overflow() {
        let engine = LegacyContextEngine;
        let mut ctx = AgentContext::new(ContextConfig::default());
        ctx.add_user_message("short message");

        let result = engine.compact(&mut ctx).await;
        assert_eq!(result.messages_removed, 0);
        assert!(result.summary.is_none());
    }

    #[tokio::test]
    async fn test_legacy_engine_compact_overflow() {
        let engine = LegacyContextEngine;
        let mut ctx = AgentContext::new(ContextConfig {
            max_tokens: 100,
            response_reserve: 50,
            count_system_prompt: true,
        });

        // Add enough messages to exceed limits
        for i in 0..20 {
            ctx.add_user_message(&format!("This is a long message number {} with lots of content to fill up tokens", i));
        }

        let result = engine.compact(&mut ctx).await;
        assert!(result.messages_removed > 0);
    }

    #[tokio::test]
    async fn test_legacy_engine_after_turn_noop() {
        let engine = LegacyContextEngine;
        let mut ctx = AgentContext::new(ContextConfig::default());
        ctx.add_user_message("Hello");

        let msg_count_before = ctx.messages().len();
        engine.after_turn(&mut ctx, "Response").await;
        assert_eq!(ctx.messages().len(), msg_count_before);
    }
}
