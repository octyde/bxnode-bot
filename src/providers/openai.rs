//! OpenAI provider implementation

use async_trait::async_trait;
use futures_util::stream::{self, BoxStream};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    CompletionRequest, CompletionResponse, FinishReason, ModelInfo, Provider, Role, Usage,
};

/// OpenAI API configuration
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
    pub organization: Option<String>,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com".to_string(),
            organization: None,
        }
    }
}

/// OpenAI provider
pub struct OpenAIProvider {
    client: Client,
    config: OpenAIConfig,
}

impl OpenAIProvider {
    pub fn new(config: OpenAIConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Convert our generic Role to OpenAI role string
    fn convert_role(role: Role) -> &'static str {
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    /// Convert OpenAI finish reason to our FinishReason
    fn convert_finish_reason(reason: &str) -> FinishReason {
        match reason {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "tool_calls" | "function_call" => FinishReason::ToolUse,
            "content_filter" => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        }
    }
}

/// OpenAI API message format
#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

/// OpenAI API request
#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

/// OpenAI API response
#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    choices: Vec<OpenAIChoice>,
    model: String,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    index: u32,
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// Streaming response chunk
#[derive(Debug, Deserialize)]
struct StreamChunk {
    id: String,
    choices: Vec<StreamChoice>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    index: u32,
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    role: Option<String>,
    content: Option<String>,
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn id(&self) -> &str {
        "openai"
    }

    fn models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                context_length: 128000,
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                context_length: 128000,
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "gpt-4-turbo".to_string(),
                name: "GPT-4 Turbo".to_string(),
                context_length: 128000,
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                context_length: 8192,
                capabilities: vec!["chat".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "gpt-3.5-turbo".to_string(),
                name: "GPT-3.5 Turbo".to_string(),
                context_length: 16385,
                capabilities: vec!["chat".to_string(), "tools".to_string()],
            },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let messages: Vec<OpenAIMessage> = request
            .messages
            .into_iter()
            .map(|msg| OpenAIMessage {
                role: Self::convert_role(msg.role).to_string(),
                content: msg.content,
            })
            .collect();

        let api_request = OpenAIRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop,
            stream: false,
        };

        let mut req = self
            .client
            .post(format!("{}/v1/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json");

        if let Some(org) = &self.config.organization {
            req = req.header("OpenAI-Organization", org);
        }

        let response = req.json(&api_request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("OpenAI API error: {}", error_text);
        }

        let api_response: OpenAIResponse = response.json().await?;

        let choice = api_response
            .choices
            .first()
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

        let usage = api_response.usage.unwrap_or(OpenAIUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(CompletionResponse {
            content: choice.message.content.clone(),
            model: api_response.model,
            finish_reason: choice
                .finish_reason
                .as_ref()
                .map(|r| Self::convert_finish_reason(r))
                .unwrap_or(FinishReason::Stop),
            usage: Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<String>>> {
        let messages: Vec<OpenAIMessage> = request
            .messages
            .into_iter()
            .map(|msg| OpenAIMessage {
                role: Self::convert_role(msg.role).to_string(),
                content: msg.content,
            })
            .collect();

        let api_request = OpenAIRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop,
            stream: true,
        };

        let mut req = self
            .client
            .post(format!("{}/v1/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json");

        if let Some(org) = &self.config.organization {
            req = req.header("OpenAI-Organization", org);
        }

        let response = req.json(&api_request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("OpenAI API error: {}", error_text);
        }

        let byte_stream = response.bytes_stream();

        let stream = byte_stream
            .map(|result| result.map_err(|e| anyhow::anyhow!("Stream error: {}", e)))
            .scan(String::new(), |buffer, result| {
                let chunks = match result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        let mut chunks = Vec::new();

                        // Process complete SSE events
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            *buffer = buffer[pos + 2..].to_string();

                            // Parse SSE event
                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data == "[DONE]" {
                                        continue;
                                    }
                                    if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                                        if let Some(choice) = chunk.choices.first() {
                                            if let Some(content) = &choice.delta.content {
                                                chunks.push(Ok(content.clone()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        chunks
                    }
                    Err(e) => vec![Err(e)],
                };
                std::future::ready(Some(stream::iter(chunks)))
            })
            .flatten();

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_role() {
        assert_eq!(OpenAIProvider::convert_role(Role::System), "system");
        assert_eq!(OpenAIProvider::convert_role(Role::User), "user");
        assert_eq!(OpenAIProvider::convert_role(Role::Assistant), "assistant");
    }

    #[test]
    fn test_convert_finish_reason() {
        assert_eq!(
            OpenAIProvider::convert_finish_reason("stop"),
            FinishReason::Stop
        );
        assert_eq!(
            OpenAIProvider::convert_finish_reason("length"),
            FinishReason::Length
        );
        assert_eq!(
            OpenAIProvider::convert_finish_reason("tool_calls"),
            FinishReason::ToolUse
        );
        assert_eq!(
            OpenAIProvider::convert_finish_reason("content_filter"),
            FinishReason::ContentFilter
        );
    }

    #[test]
    fn test_default_config() {
        let config = OpenAIConfig::default();
        assert_eq!(config.base_url, "https://api.openai.com");
        assert!(config.api_key.is_empty());
        assert!(config.organization.is_none());
    }

    #[test]
    fn test_provider_id() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config);
        assert_eq!(provider.id(), "openai");
    }

    #[test]
    fn test_models_list() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config);
        let models = provider.models();

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("gpt-4")));
    }

    #[test]
    fn test_openai_message_serialization() {
        let msg = OpenAIMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
    }
}
