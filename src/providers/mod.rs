//! Providers module - LLM provider integrations
//!
//! Supported providers:
//! - Anthropic (Claude)
//! - OpenAI (GPT)
//! - AWS Bedrock
//! - Ollama (local)
//! - Z.AI
//! - Venice.ai
//! - Qwen Portal

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
pub mod registry;

pub use registry::ProviderRegistry;
