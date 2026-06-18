//! Anthropic (Claude) provider implementation

use async_trait::async_trait;
use futures_util::stream::{self, BoxStream};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    CompletionRequest, CompletionResponse, FinishReason, ModelInfo, Provider, Role,
    ToolCallResponse, Usage,
};

/// Anthropic API configuration
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub base_url: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.anthropic.com".to_string(),
        }
    }
}

/// Anthropic provider
pub struct AnthropicProvider {
    client: Client,
    config: AnthropicConfig,
}

impl AnthropicProvider {
    pub fn new(config: AnthropicConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Convert our generic Role to Anthropic role string
    fn convert_role(role: Role) -> &'static str {
        match role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "user", // System messages handled separately
            Role::Tool => "user",   // Anthropic tool results are user-role blocks
        }
    }

    /// Convert Anthropic stop reason to our FinishReason
    fn convert_stop_reason(reason: &str) -> FinishReason {
        match reason {
            "end_turn" | "stop_sequence" => FinishReason::Stop,
            "max_tokens" => FinishReason::Length,
            "tool_use" => FinishReason::ToolUse,
            _ => FinishReason::Stop,
        }
    }
}

/// Anthropic API message format
#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Anthropic tool definition
#[derive(Debug, Serialize)]
struct AnthropicToolDefinition {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

/// Anthropic API request
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicToolDefinition>,
}

/// Anthropic API response
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    content: Vec<ContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
    /// Tool use fields
    id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

/// Streaming event types
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MessageStartData },
    #[serde(rename = "content_block_start")]
    ContentBlockStart { index: usize, content_block: ContentBlock },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: DeltaBlock },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: MessageDelta, usage: Option<DeltaUsage> },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: ApiError },
}

#[derive(Debug, Deserialize)]
struct MessageStartData {
    id: String,
    model: String,
}

#[derive(Debug, Deserialize)]
struct DeltaBlock {
    #[serde(rename = "type")]
    delta_type: String,
    text: Option<String>,
    /// Partial JSON for tool input (type = "input_json_delta")
    partial_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageDelta {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeltaUsage {
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                context_length: 200000,
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                context_length: 200000,
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "claude-3-sonnet-20240229".to_string(),
                name: "Claude 3 Sonnet".to_string(),
                context_length: 200000,
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "claude-3-haiku-20240307".to_string(),
                name: "Claude 3 Haiku".to_string(),
                context_length: 200000,
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string()],
            },
        ]
    }

    async fn complete(&self, request: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        // Extract system message if present
        let mut system_prompt = None;
        let mut messages = Vec::new();

        for msg in request.messages {
            if msg.role == Role::System {
                system_prompt = Some(msg.content);
            } else {
                messages.push(AnthropicMessage {
                    role: Self::convert_role(msg.role).to_string(),
                    content: msg.content,
                });
            }
        }

        // Convert tool definitions to Anthropic format
        let tools: Vec<AnthropicToolDefinition> = request
            .tools
            .iter()
            .map(|t| AnthropicToolDefinition {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        // Strip provider prefix (e.g. "anthropic/claude-3-opus" → "claude-3-opus")
        let model_name = request.model.split_once('/').map_or(request.model.as_str(), |(_,m)| m);

        let api_request = AnthropicRequest {
            model: model_name.to_string(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system: system_prompt,
            temperature: request.temperature,
            stop_sequences: request.stop,
            stream: false,
            tools,
        };

        let response = self
            .client
            .post(format!("{}/v1/messages", self.config.base_url))
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&api_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Anthropic API error: {}", error_text);
        }

        let api_response: AnthropicResponse = response.json().await?;

        // Extract text content
        let content = api_response
            .content
            .iter()
            .filter_map(|block| {
                if block.content_type == "text" {
                    block.text.clone()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        // Extract tool_use content blocks
        let tool_calls: Vec<ToolCallResponse> = api_response
            .content
            .iter()
            .filter_map(|block| {
                if block.content_type == "tool_use" {
                    Some(ToolCallResponse {
                        id: block.id.clone().unwrap_or_default(),
                        name: block.name.clone().unwrap_or_default(),
                        arguments: block.input.clone().unwrap_or(serde_json::json!({})),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(CompletionResponse {
            content,
            model: api_response.model,
            finish_reason: api_response
                .stop_reason
                .map(|r| Self::convert_stop_reason(&r))
                .unwrap_or(FinishReason::Stop),
            usage: Usage {
                prompt_tokens: api_response.usage.input_tokens,
                completion_tokens: api_response.usage.output_tokens,
                total_tokens: api_response.usage.input_tokens + api_response.usage.output_tokens,
            },
            tool_calls,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<String>>> {
        // Extract system message if present
        let mut system_prompt = None;
        let mut messages = Vec::new();

        for msg in request.messages {
            if msg.role == Role::System {
                system_prompt = Some(msg.content);
            } else {
                messages.push(AnthropicMessage {
                    role: Self::convert_role(msg.role).to_string(),
                    content: msg.content,
                });
            }
        }

        // Convert tool definitions to Anthropic format
        let tools: Vec<AnthropicToolDefinition> = request
            .tools
            .iter()
            .map(|t| AnthropicToolDefinition {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        // Strip provider prefix (e.g. "anthropic/claude-3-opus" → "claude-3-opus")
        let model_name = request.model.split_once('/').map_or(request.model.as_str(), |(_,m)| m);

        let api_request = AnthropicRequest {
            model: model_name.to_string(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system: system_prompt,
            temperature: request.temperature,
            stop_sequences: request.stop,
            stream: true,
            tools,
        };

        let response = self
            .client
            .post(format!("{}/v1/messages", self.config.base_url))
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&api_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Anthropic API error: {}", error_text);
        }

        let byte_stream = response.bytes_stream();

        // State for accumulating tool_use content blocks
        // Anthropic streams: content_block_start (id, name) → content_block_delta (input_json_delta) → content_block_stop
        #[derive(Default)]
        struct ToolUseAccumulator {
            current_id: String,
            current_name: String,
            current_input: String,
            in_tool_use: bool,
            completed: Vec<(String, String, String)>, // (id, name, input_json)
        }

        let stream = byte_stream
            .map(|result| {
                result.map_err(|e| anyhow::anyhow!("Stream error: {}", e))
            })
            .scan((String::new(), ToolUseAccumulator::default()), |(buffer, tool_acc), result| {
                let chunks = match result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        let mut chunks = Vec::new();

                        // Process complete SSE events
                        while let Some(pos) = buffer.find("\n\n") {
                            let event_text = buffer[..pos].to_string();
                            *buffer = buffer[pos + 2..].to_string();

                            // Anthropic SSE has "event: type\ndata: json" format
                            let mut data_str = None;
                            for line in event_text.lines() {
                                if let Some(d) = line.strip_prefix("data: ") {
                                    data_str = Some(d.to_string());
                                }
                            }

                            if let Some(data) = data_str {
                                if data == "[DONE]" {
                                    continue;
                                }
                                if let Ok(event) = serde_json::from_str::<StreamEvent>(&data) {
                                    match event {
                                        StreamEvent::ContentBlockStart { content_block, .. } => {
                                            if content_block.content_type == "tool_use" {
                                                tool_acc.in_tool_use = true;
                                                tool_acc.current_id = content_block.id.unwrap_or_default();
                                                tool_acc.current_name = content_block.name.unwrap_or_default();
                                                tool_acc.current_input.clear();
                                            }
                                        }
                                        StreamEvent::ContentBlockDelta { delta, .. } => {
                                            if tool_acc.in_tool_use {
                                                if let Some(partial) = delta.partial_json {
                                                    tool_acc.current_input.push_str(&partial);
                                                }
                                            } else if let Some(text) = delta.text {
                                                chunks.push(Ok(text));
                                            }
                                        }
                                        StreamEvent::ContentBlockStop { .. } => {
                                            if tool_acc.in_tool_use {
                                                tool_acc.completed.push((
                                                    tool_acc.current_id.clone(),
                                                    tool_acc.current_name.clone(),
                                                    tool_acc.current_input.clone(),
                                                ));
                                                tool_acc.in_tool_use = false;
                                            }
                                        }
                                        StreamEvent::MessageDelta { .. } | StreamEvent::MessageStop => {
                                            // Emit accumulated tool calls as XML
                                            if !tool_acc.completed.is_empty() {
                                                let mut xml = String::new();
                                                for (id, name, input) in &tool_acc.completed {
                                                    let input_json = if input.is_empty() { "{}" } else { input.as_str() };
                                                    xml.push_str(&format!(
                                                        "<tool_call>{{\"id\":\"{}\",\"name\":\"{}\",\"arguments\":{}}}</tool_call>",
                                                        id, name, input_json
                                                    ));
                                                }
                                                chunks.push(Ok(xml));
                                                tool_acc.completed.clear();
                                            }
                                        }
                                        _ => {}
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
        assert_eq!(AnthropicProvider::convert_role(Role::User), "user");
        assert_eq!(AnthropicProvider::convert_role(Role::Assistant), "assistant");
        assert_eq!(AnthropicProvider::convert_role(Role::System), "user");
    }

    #[test]
    fn test_convert_stop_reason() {
        assert_eq!(
            AnthropicProvider::convert_stop_reason("end_turn"),
            FinishReason::Stop
        );
        assert_eq!(
            AnthropicProvider::convert_stop_reason("max_tokens"),
            FinishReason::Length
        );
        assert_eq!(
            AnthropicProvider::convert_stop_reason("tool_use"),
            FinishReason::ToolUse
        );
    }

    #[test]
    fn test_default_config() {
        let config = AnthropicConfig::default();
        assert_eq!(config.base_url, "https://api.anthropic.com");
        assert!(config.api_key.is_empty());
    }

    #[test]
    fn test_provider_id() {
        let config = AnthropicConfig::default();
        let provider = AnthropicProvider::new(config);
        assert_eq!(provider.id(), "anthropic");
    }

    #[test]
    fn test_models_list() {
        let config = AnthropicConfig::default();
        let provider = AnthropicProvider::new(config);
        let models = provider.models();

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("claude-3")));
    }
}
