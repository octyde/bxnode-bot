//! Providers module - LLM provider integrations
//!
//! Supported providers:
//! - Anthropic (Claude)
//! - OpenAI (GPT)
//! - Ollama (local)
//! - Z.AI (Zhipu GLM)
//! - Groq
//! - DeepSeek
//! - Mistral
//! - Venice.ai
//! - Qwen (DashScope)
//! - Google Gemini
//!
//! Planned:
//! - AWS Bedrock

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};

/// Chat message for provider requests.
///
/// Beyond plain `role` + `content`, a message can carry tool-calling structure:
/// an **assistant** message may include `tool_calls` (the calls the model made),
/// and a **tool** message carries `tool_call_id` (which call it answers). This
/// is what lets the OpenAI/Z.AI message sequence stay valid across a tool round
/// (assistant-with-tool_calls → tool results → assistant), instead of jamming
/// results into a fake user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Tool calls this assistant message made (empty for non-tool turns).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallResponse>,
    /// The id of the tool call this `Tool`-role message answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// A plain text message (no tool structure).
    pub fn text(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// An assistant message that made `tool_calls` (content may be empty).
    pub fn assistant_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCallResponse>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls,
            tool_call_id: None,
        }
    }

    /// A tool-result message answering the call `tool_call_id`.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// A tool-result message (carries `tool_call_id`).
    Tool,
}

/// Tool definition for provider requests (matches OpenAI function calling format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinitionRequest {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema (JSON Schema)
    pub input_schema: serde_json::Value,
}

/// Tool call parsed from provider response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    /// Tool call ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Tool arguments as JSON
    pub arguments: serde_json::Value,
}

/// Completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model identifier
    pub model: String,

    /// Messages history
    pub messages: Vec<Message>,

    /// Temperature (0.0 - 2.0)
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Maximum tokens to generate
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Stop sequences
    #[serde(default)]
    pub stop: Vec<String>,

    /// Whether to stream the response
    #[serde(default)]
    pub stream: bool,

    /// Available tools for function calling
    #[serde(default)]
    pub tools: Vec<ToolDefinitionRequest>,
}

/// Completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Generated content
    pub content: String,

    /// Model used
    pub model: String,

    /// Finish reason
    pub finish_reason: FinishReason,

    /// Token usage
    pub usage: Usage,

    /// Tool calls from the model (if any)
    #[serde(default)]
    pub tool_calls: Vec<ToolCallResponse>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolUse,
    ContentFilter,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Provider trait - implement this for each LLM provider
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider identifier
    fn id(&self) -> &str;

    /// List available models
    fn models(&self) -> Vec<ModelInfo>;

    /// Create a completion
    async fn complete(&self, request: CompletionRequest) -> anyhow::Result<CompletionResponse>;

    /// Create a streaming completion
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<String>>>;
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_length: u32,
    pub capabilities: Vec<String>,
}

// Provider implementations
pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod openai_compatible;
pub mod registry;

pub use registry::ProviderRegistry;
