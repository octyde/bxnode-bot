//! Agent context management
//!
//! Handles message history, token counting, and context window management.

use crate::providers::{Message, Role};
use serde::{Deserialize, Serialize};

/// Context configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Maximum context tokens
    pub max_tokens: u32,

    /// Reserve tokens for response
    pub response_reserve: u32,

    /// Whether to include system prompt in token count
    pub count_system_prompt: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 128_000,
            response_reserve: 4096,
            count_system_prompt: true,
        }
    }
}

/// Agent context - manages conversation history and context window
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// System prompt
    system_prompt: Option<String>,

    /// Message history
    messages: Vec<Message>,

    /// Configuration
    config: ContextConfig,
}

impl AgentContext {
    /// Create a new context
    pub fn new(config: ContextConfig) -> Self {
        Self {
            system_prompt: None,
            messages: Vec::new(),
            config,
        }
    }

    /// Create context with a system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the system prompt
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = Some(prompt.into());
    }

    /// Get the system prompt
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.messages.push(Message {
            role: Role::User,
            content: content.into(),
        });
    }

    /// Add an assistant message
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.messages.push(Message {
            role: Role::Assistant,
            content: content.into(),
        });
    }

    /// Add a message with specified role
    pub fn add_message(&mut self, role: Role, content: impl Into<String>) {
        self.messages.push(Message {
            role,
            content: content.into(),
        });
    }

    /// Get all messages (without system prompt)
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get messages for sending to provider (includes system prompt as first message if present)
    pub fn messages_for_provider(&self) -> Vec<Message> {
        let mut result = Vec::with_capacity(self.messages.len() + 1);

        if let Some(ref system) = self.system_prompt {
            result.push(Message {
                role: Role::System,
                content: system.clone(),
            });
        }

        result.extend(self.messages.clone());
        result
    }

    /// Get the last N messages
    pub fn last_n_messages(&self, n: usize) -> &[Message] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }

    /// Clear all messages (keeps system prompt)
    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// Clear everything including system prompt
    pub fn clear_all(&mut self) {
        self.system_prompt = None;
        self.messages.clear();
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Estimate token count (rough approximation: ~4 chars per token)
    pub fn estimate_tokens(&self) -> u32 {
        let mut total = 0u32;

        if let Some(ref system) = self.system_prompt {
            if self.config.count_system_prompt {
                total += estimate_string_tokens(system);
            }
        }

        for msg in &self.messages {
            total += estimate_string_tokens(&msg.content);
            total += 4; // Role overhead
        }

        total
    }

    /// Check if context is within limits
    pub fn within_limits(&self) -> bool {
        let tokens = self.estimate_tokens();
        tokens + self.config.response_reserve <= self.config.max_tokens
    }

    /// Available tokens for response
    pub fn available_tokens(&self) -> u32 {
        let used = self.estimate_tokens();
        self.config
            .max_tokens
            .saturating_sub(used)
            .saturating_sub(self.config.response_reserve)
    }

    /// Truncate oldest messages to fit within limits
    pub fn truncate_to_fit(&mut self) {
        while !self.within_limits() && !self.messages.is_empty() {
            self.messages.remove(0);
        }
    }

    /// Get a summary of the context state
    pub fn summary(&self) -> ContextSummary {
        ContextSummary {
            message_count: self.messages.len(),
            estimated_tokens: self.estimate_tokens(),
            max_tokens: self.config.max_tokens,
            available_tokens: self.available_tokens(),
            has_system_prompt: self.system_prompt.is_some(),
        }
    }
}

impl Default for AgentContext {
    fn default() -> Self {
        Self::new(ContextConfig::default())
    }
}

/// Summary of context state
#[derive(Debug, Clone, Serialize)]
pub struct ContextSummary {
    pub message_count: usize,
    pub estimated_tokens: u32,
    pub max_tokens: u32,
    pub available_tokens: u32,
    pub has_system_prompt: bool,
}

/// Estimate tokens for a string (rough approximation)
fn estimate_string_tokens(s: &str) -> u32 {
    // Rough estimate: ~4 characters per token for English text
    // This is a simplification; real tokenization varies by model
    (s.len() as f64 / 4.0).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = AgentContext::default();
        assert!(ctx.system_prompt().is_none());
        assert_eq!(ctx.message_count(), 0);
    }

    #[test]
    fn test_context_with_system_prompt() {
        let ctx = AgentContext::default().with_system_prompt("You are a helpful assistant.");
        assert_eq!(
            ctx.system_prompt(),
            Some("You are a helpful assistant.")
        );
    }

    #[test]
    fn test_add_messages() {
        let mut ctx = AgentContext::default();

        ctx.add_user_message("Hello");
        ctx.add_assistant_message("Hi there!");
        ctx.add_user_message("How are you?");

        assert_eq!(ctx.message_count(), 3);

        let messages = ctx.messages();
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[2].role, Role::User);
    }

    #[test]
    fn test_messages_for_provider() {
        let mut ctx = AgentContext::default().with_system_prompt("System prompt");

        ctx.add_user_message("User message");

        let provider_messages = ctx.messages_for_provider();
        assert_eq!(provider_messages.len(), 2);
        assert_eq!(provider_messages[0].role, Role::System);
        assert_eq!(provider_messages[1].role, Role::User);
    }

    #[test]
    fn test_last_n_messages() {
        let mut ctx = AgentContext::default();

        for i in 0..10 {
            ctx.add_user_message(format!("Message {}", i));
        }

        let last3 = ctx.last_n_messages(3);
        assert_eq!(last3.len(), 3);
        assert_eq!(last3[0].content, "Message 7");
        assert_eq!(last3[2].content, "Message 9");
    }

    #[test]
    fn test_clear_messages() {
        let mut ctx = AgentContext::default().with_system_prompt("System");

        ctx.add_user_message("Hello");
        ctx.add_assistant_message("Hi");

        ctx.clear_messages();
        assert_eq!(ctx.message_count(), 0);
        assert!(ctx.system_prompt().is_some()); // System prompt preserved

        ctx.clear_all();
        assert!(ctx.system_prompt().is_none());
    }

    #[test]
    fn test_estimate_tokens() {
        let mut ctx = AgentContext::default();

        // Empty context should have minimal tokens
        assert!(ctx.estimate_tokens() < 10);

        // Add some content
        ctx.add_user_message("This is a test message with some content.");
        let tokens = ctx.estimate_tokens();
        assert!(tokens > 10);
    }

    #[test]
    fn test_truncate_to_fit() {
        let config = ContextConfig {
            max_tokens: 100,
            response_reserve: 20,
            count_system_prompt: true,
        };

        let mut ctx = AgentContext::new(config);

        // Add messages until over limit
        for i in 0..50 {
            ctx.add_user_message(format!(
                "This is message number {} with some content",
                i
            ));
        }

        let original_count = ctx.message_count();
        ctx.truncate_to_fit();

        assert!(ctx.message_count() < original_count);
        assert!(ctx.within_limits());
    }

    #[test]
    fn test_context_summary() {
        let mut ctx = AgentContext::default().with_system_prompt("System");

        ctx.add_user_message("Hello");
        ctx.add_assistant_message("Hi");

        let summary = ctx.summary();
        assert_eq!(summary.message_count, 2);
        assert!(summary.has_system_prompt);
        assert!(summary.estimated_tokens > 0);
    }

    #[test]
    fn test_estimate_string_tokens() {
        // Empty string
        assert_eq!(estimate_string_tokens(""), 0);

        // Short string
        assert!(estimate_string_tokens("Hi") >= 1);

        // Longer string
        let long_text = "a".repeat(100);
        let tokens = estimate_string_tokens(&long_text);
        assert!(tokens >= 20 && tokens <= 30); // ~25 tokens for 100 chars
    }
}
