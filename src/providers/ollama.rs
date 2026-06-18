//! Ollama (local) provider implementation

use async_trait::async_trait;
use futures_util::stream::{self, BoxStream};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    CompletionRequest, CompletionResponse, FinishReason, ModelInfo, Provider, Role, StreamEvent,
    Usage,
};

/// Ollama API configuration
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
        }
    }
}

/// Ollama provider
pub struct OllamaProvider {
    client: Client,
    config: OllamaConfig,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Fetch available models from Ollama
    pub async fn fetch_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to fetch Ollama models");
        }

        let tags: OllamaTagsResponse = response.json().await?;

        Ok(tags
            .models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.name.clone(),
                name: m.name,
                context_length: 4096, // Ollama doesn't provide this, use default
                capabilities: vec!["chat".to_string()],
            })
            .collect())
    }

    /// Convert our generic Role to Ollama role string
    fn convert_role(role: Role) -> &'static str {
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }

    /// Convert Ollama done_reason to our FinishReason
    fn convert_done_reason(reason: Option<&str>) -> FinishReason {
        match reason {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            _ => FinishReason::Stop,
        }
    }
}

/// Ollama tags response
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    modified_at: String,
    #[serde(default)]
    size: u64,
}

/// Ollama chat message
#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

/// Ollama chat request
#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
}

/// Ollama chat response
#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

/// Streaming chunk
#[derive(Debug, Deserialize)]
struct StreamingChunk {
    model: String,
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
}

#[async_trait]
impl Provider for OllamaProvider {
    fn id(&self) -> &str {
        "ollama"
    }

    fn models(&self) -> Vec<ModelInfo> {
        // Return common Ollama models as static list
        // For dynamic list, use fetch_models()
        vec![
            ModelInfo {
                id: "llama3.2".to_string(),
                name: "Llama 3.2".to_string(),
                context_length: 128000,
                capabilities: vec!["chat".to_string()],
            },
            ModelInfo {
                id: "llama3.1".to_string(),
                name: "Llama 3.1".to_string(),
                context_length: 128000,
                capabilities: vec!["chat".to_string()],
            },
            ModelInfo {
                id: "mistral".to_string(),
                name: "Mistral".to_string(),
                context_length: 32000,
                capabilities: vec!["chat".to_string()],
            },
            ModelInfo {
                id: "mixtral".to_string(),
                name: "Mixtral".to_string(),
                context_length: 32000,
                capabilities: vec!["chat".to_string()],
            },
            ModelInfo {
                id: "codellama".to_string(),
                name: "Code Llama".to_string(),
                context_length: 16000,
                capabilities: vec!["chat".to_string(), "code".to_string()],
            },
            ModelInfo {
                id: "qwen2.5-coder".to_string(),
                name: "Qwen 2.5 Coder".to_string(),
                context_length: 32000,
                capabilities: vec!["chat".to_string(), "code".to_string()],
            },
            ModelInfo {
                id: "deepseek-coder-v2".to_string(),
                name: "DeepSeek Coder V2".to_string(),
                context_length: 128000,
                capabilities: vec!["chat".to_string(), "code".to_string()],
            },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let messages: Vec<OllamaMessage> = request
            .messages
            .into_iter()
            .map(|msg| OllamaMessage {
                role: Self::convert_role(msg.role).to_string(),
                content: msg.content,
            })
            .collect();

        let options = if request.temperature.is_some()
            || request.max_tokens.is_some()
            || !request.stop.is_empty()
        {
            Some(OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
                stop: request.stop,
            })
        } else {
            None
        };

        // Strip provider prefix (e.g. "ollama/llama3" → "llama3")
        let model_name = request.model.split_once('/').map_or(request.model.as_str(), |(_,m)| m);

        let api_request = OllamaChatRequest {
            model: model_name.to_string(),
            messages,
            stream: false,
            options,
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.config.base_url))
            .json(&api_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Ollama API error: {}", error_text);
        }

        let api_response: OllamaChatResponse = response.json().await?;

        let prompt_tokens = api_response.prompt_eval_count.unwrap_or(0);
        let completion_tokens = api_response.eval_count.unwrap_or(0);

        Ok(CompletionResponse {
            content: api_response.message.content,
            model: api_response.model,
            finish_reason: Self::convert_done_reason(api_response.done_reason.as_deref()),
            usage: Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            tool_calls: vec![],
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
        let messages: Vec<OllamaMessage> = request
            .messages
            .into_iter()
            .map(|msg| OllamaMessage {
                role: Self::convert_role(msg.role).to_string(),
                content: msg.content,
            })
            .collect();

        let options = if request.temperature.is_some()
            || request.max_tokens.is_some()
            || !request.stop.is_empty()
        {
            Some(OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
                stop: request.stop,
            })
        } else {
            None
        };

        // Strip provider prefix (e.g. "ollama/llama3" → "llama3")
        let model_name = request.model.split_once('/').map_or(request.model.as_str(), |(_,m)| m);

        let api_request = OllamaChatRequest {
            model: model_name.to_string(),
            messages,
            stream: true,
            options,
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.config.base_url))
            .json(&api_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Ollama API error: {}", error_text);
        }

        let byte_stream = response.bytes_stream();

        let stream = byte_stream
            .map(|result| result.map_err(|e| anyhow::anyhow!("Stream error: {}", e)))
            .scan(String::new(), |buffer, result| {
                let chunks = match result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        let mut chunks = Vec::new();

                        // Ollama streams JSON objects separated by newlines
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].to_string();
                            *buffer = buffer[pos + 1..].to_string();

                            if line.trim().is_empty() {
                                continue;
                            }

                            if let Ok(chunk) = serde_json::from_str::<StreamingChunk>(&line) {
                                if !chunk.message.content.is_empty() {
                                    chunks.push(Ok(StreamEvent::TextDelta(chunk.message.content)));
                                }
                                if chunk.done {
                                    chunks.push(Ok(StreamEvent::Done(Some(
                                        Self::convert_done_reason(chunk.done_reason.as_deref()),
                                    ))));
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
        assert_eq!(OllamaProvider::convert_role(Role::System), "system");
        assert_eq!(OllamaProvider::convert_role(Role::User), "user");
        assert_eq!(OllamaProvider::convert_role(Role::Assistant), "assistant");
    }

    #[test]
    fn test_convert_done_reason() {
        assert_eq!(
            OllamaProvider::convert_done_reason(Some("stop")),
            FinishReason::Stop
        );
        assert_eq!(
            OllamaProvider::convert_done_reason(Some("length")),
            FinishReason::Length
        );
        assert_eq!(
            OllamaProvider::convert_done_reason(None),
            FinishReason::Stop
        );
    }

    #[test]
    fn test_default_config() {
        let config = OllamaConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
    }

    #[test]
    fn test_provider_id() {
        let config = OllamaConfig::default();
        let provider = OllamaProvider::new(config);
        assert_eq!(provider.id(), "ollama");
    }

    #[test]
    fn test_models_list() {
        let config = OllamaConfig::default();
        let provider = OllamaProvider::new(config);
        let models = provider.models();

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("llama")));
    }

    #[test]
    fn test_ollama_message_serialization() {
        let msg = OllamaMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
    }

    #[test]
    fn test_ollama_request_serialization() {
        let request = OllamaChatRequest {
            model: "llama3".to_string(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            stream: true, // stream: true to test serialization (false is skipped)
            options: Some(OllamaOptions {
                temperature: Some(0.7),
                num_predict: Some(100),
                stop: vec![],
            }),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "llama3");
        assert_eq!(json["stream"], true);
        assert!(json["options"]["temperature"].is_number());
    }
}
