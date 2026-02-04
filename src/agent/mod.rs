//! Agent module - AI agent execution runtime
//!
//! This module implements the core agent loop:
//! 1. Receive message
//! 2. Build context (history, tools, system prompt)
//! 3. Call LLM provider
//! 4. Execute tool calls (if any)
//! 5. Return response

use serde::{Deserialize, Serialize};

pub mod context;
pub mod execution;
pub mod tools;

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent identifier
    pub id: String,

    /// Display name
    pub name: String,

    /// System prompt
    pub system_prompt: Option<String>,

    /// Primary model ID
    pub model: String,

    /// Enabled tools
    #[serde(default)]
    pub tools: Vec<String>,

    /// Maximum context tokens
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u32,

    /// Temperature
    #[serde(default)]
    pub temperature: Option<f32>,
}

fn default_max_context_tokens() -> u32 {
    128_000
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            name: "Default Agent".to_string(),
            system_prompt: None,
            model: "anthropic/claude-3-opus".to_string(),
            tools: vec![],
            max_context_tokens: default_max_context_tokens(),
            temperature: None,
        }
    }
}

/// Agent state
#[derive(Debug, Clone, Default)]
pub struct AgentState {
    /// Current session ID
    pub session_id: Option<String>,

    /// Message history
    pub messages: Vec<crate::providers::Message>,
}
