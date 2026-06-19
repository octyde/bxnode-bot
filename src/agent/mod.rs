//! Agent module - AI agent execution runtime
//!
//! This module implements the core agent loop for processing user messages
//! and generating AI-powered responses.
//!
//! # Agent Loop Architecture
//!
//! The agent follows a standard agentic loop pattern:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    Agent Loop                           │
//! ├─────────────────────────────────────────────────────────┤
//! │  1. Receive message from channel                        │
//! │         ↓                                               │
//! │  2. Build context                                       │
//! │     - Load conversation history                         │
//! │     - Apply system prompt                               │
//! │     - Attach available tools                            │
//! │         ↓                                               │
//! │  3. Call LLM provider                                   │
//! │     - Stream or batch response                          │
//! │         ↓                                               │
//! │  4. Check for tool calls                                │
//! │     ├─ No tools → Return response                       │
//! │     └─ Has tools → Execute tools, go to step 3          │
//! │         ↓                                               │
//! │  5. Return final response to channel                    │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Key Components
//!
//! - [`AgentContext`] - Manages conversation history and token limits
//! - [`AgentExecutor`] - Runs the agent loop with a provider
//! - [`ToolRegistry`] - Registry of available tools for the agent
//!
//! # Usage
//!
//! ```ignore
//! use bxnode_bot::agent::{AgentConfig, AgentContext, AgentExecutor, ToolRegistry};
//!
//! // Create executor with provider and tools
//! let executor = AgentExecutor::new(provider, tools, config);
//!
//! // Create context for conversation
//! let mut context = AgentContext::default();
//!
//! // Execute a turn
//! let result = executor.execute(&mut context, "Hello!").await?;
//! println!("Response: {}", result.content);
//! ```
//!
//! # Streaming
//!
//! The agent supports streaming responses via `execute_stream`:
//!
//! ```ignore
//! executor.execute_stream(&mut context, "Hello!", |event| {
//!     match event {
//!         AgentEvent::TextDelta { content } => print!("{}", content),
//!         AgentEvent::ToolCall { call } => println!("Calling: {}", call.name),
//!         AgentEvent::Completed { .. } => println!("\nDone!"),
//!         _ => {}
//!     }
//! }).await?;
//! ```

use serde::{Deserialize, Serialize};

pub mod coding_tools;
pub mod context;
pub mod context_engine;
pub mod diff_tool;
pub mod execution;
pub mod image_tool;
pub mod pdf_tool;
pub mod tool_policy;
pub mod tools;
pub mod tts_tool;
pub mod web_tools;

#[cfg(test)]
mod tools_tests;

// Re-export public API
pub use context::{AgentContext, ContextConfig};
pub use execution::{AgentEvent, AgentExecutor, ExecutionResult, ExecutionUsage};
pub use tool_policy::{
    ToolMetadata, ToolPermissionDecision, ToolPermissionMode, ToolPolicy, ToolRisk,
};
pub use tools::{Tool, ToolCall, ToolDefinition, ToolRegistry, ToolResult};

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

    /// Maximum OUTPUT tokens per model turn (the `max_tokens` request field).
    /// Must be large enough for a reasoning model (e.g. GLM-5.2) to emit its
    /// `reasoning_content` AND still produce tool calls / text in the same
    /// turn. Too small (the old hardcoded 4096) made the model spend the whole
    /// budget on reasoning, hit `finish_reason: length`, and end the turn with
    /// ZERO tool calls — so an agent asked to write files just stalled.
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,

    /// Temperature
    #[serde(default)]
    pub temperature: Option<f32>,
}

fn default_max_context_tokens() -> u32 {
    128_000
}

fn default_max_output_tokens() -> u32 {
    16_384
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
            max_output_tokens: default_max_output_tokens(),
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
