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

/// Chat message for provider requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
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
