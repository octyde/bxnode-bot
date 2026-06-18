//! Generic OpenAI-compatible provider implementation
//!
//! This module provides a reusable provider for all services that implement
//! the OpenAI `/v1/chat/completions` API with Bearer token authentication.

use async_trait::async_trait;
use futures_util::stream::{self, BoxStream};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    CompletionRequest, CompletionResponse, FinishReason, Message, ModelInfo, Provider, Role,
    StreamEvent, ToolCallResponse, Usage,
};

/// Configuration for an OpenAI-compatible provider
#[derive(Debug, Clone)]
pub struct OpenAICompatibleConfig {
    /// Provider identifier (e.g., "groq", "deepseek")
    pub provider_id: String,
    /// API key for authentication
    pub api_key: String,
    /// Base URL for the API
    pub base_url: String,
    /// Model catalog
    pub models: Vec<ModelInfo>,
    /// Path to the chat completions endpoint (default: "/v1/chat/completions")
    pub completions_path: Option<String>,
    /// Extra headers to include in requests (e.g., OpenAI-Organization)
    pub extra_headers: Vec<(String, String)>,
    /// Extra body parameters to include in requests (e.g., tool_stream for Z.AI)
    pub extra_body: Vec<(String, serde_json::Value)>,
}

/// Generic OpenAI-compatible provider
///
/// Supports any LLM API that uses the OpenAI `/v1/chat/completions` protocol
/// with Bearer token authentication.
pub struct OpenAICompatibleProvider {
    client: Client,
    provider_id: String,
    api_key: String,
    base_url: String,
    completions_path: String,
    extra_headers: Vec<(String, String)>,
    extra_body: Vec<(String, serde_json::Value)>,
    models: Vec<ModelInfo>,
}

impl OpenAICompatibleProvider {
    pub fn new(config: OpenAICompatibleConfig) -> Self {
        Self {
            client: Client::new(),
            provider_id: config.provider_id,
            api_key: config.api_key,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            completions_path: config
                .completions_path
                .unwrap_or_else(|| "/v1/chat/completions".to_string()),
            extra_headers: config.extra_headers,
            extra_body: config.extra_body,
            models: config.models,
        }
    }

    /// Convert our generic Role to OpenAI role string
    fn convert_role(role: Role) -> &'static str {
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }

    /// Serialize one message into the OpenAI/Z.AI wire shape, including tool
    /// structure: an assistant message carries `tool_calls`, a tool message
    /// carries `tool_call_id`. Emitting these correctly is what keeps a tool
    /// round's message sequence valid (otherwise Z.AI rejects it as illegal).
    fn message_to_json(msg: &Message) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert("role".into(), Self::convert_role(msg.role).into());
        // `content` must be present (use "" rather than null for a pure
        // tool-call assistant turn — Z.AI rejects a missing content key).
        obj.insert("content".into(), msg.content.clone().into());
        if !msg.tool_calls.is_empty() {
            let calls: Vec<serde_json::Value> = msg
                .tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            // OpenAI wants arguments as a JSON *string*.
                            "arguments": serde_json::to_string(&tc.arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        }
                    })
                })
                .collect();
            obj.insert("tool_calls".into(), serde_json::Value::Array(calls));
        }
        if let Some(id) = &msg.tool_call_id {
            obj.insert("tool_call_id".into(), id.clone().into());
        }
        serde_json::Value::Object(obj)
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
    /// Content can be null when model returns tool_calls only
    #[serde(default)]
    content: Option<String>,
    /// Reasoning/thinking content (used by GLM-5, DeepSeek-R1, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
    /// Tool calls from the assistant
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OpenAIToolCall>,
}

/// OpenAI tool call in response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    tool_type: String,
    function: OpenAIFunctionCall,
}

fn default_tool_type() -> String {
    "function".to_string()
}

/// OpenAI function call payload
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

/// OpenAI tool definition (function calling format)
#[derive(Debug, Serialize)]
struct OpenAIToolDefinition {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunctionDefinition,
}

/// OpenAI function definition
#[derive(Debug, Serialize)]
struct OpenAIFunctionDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Tool result message (role = "tool")
#[derive(Debug, Serialize)]
struct OpenAIToolResultMessage {
    role: String,
    tool_call_id: String,
    content: String,
}

/// OpenAI API request
#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAIToolDefinition>,
}

/// OpenAI API response
#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    #[allow(dead_code)]
    id: String,
    choices: Vec<OpenAIChoice>,
    model: String,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    id: String,
    choices: Vec<StreamChoice>,
    #[allow(dead_code)]
    model: String,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[allow(dead_code)]
    index: u32,
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<String>,
    /// Reasoning/thinking content (used by GLM-5, DeepSeek-R1, etc.)
    reasoning_content: Option<String>,
    /// Tool calls (streamed incrementally)
    #[serde(default)]
    tool_calls: Vec<StreamToolCall>,
}

/// Streaming tool call chunk
#[derive(Debug, Deserialize)]
struct StreamToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunctionCall>,
}

/// Streaming function call chunk
#[derive(Debug, Deserialize)]
struct StreamFunctionCall {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[async_trait]
impl Provider for OpenAICompatibleProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }

    fn models(&self) -> Vec<ModelInfo> {
        self.models.clone()
    }

    async fn complete(&self, request: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let messages: Vec<serde_json::Value> = request
            .messages
            .into_iter()
            .map(|msg| Self::message_to_json(&msg))
            .collect();

        // Convert tool definitions to OpenAI format
        let tools: Vec<OpenAIToolDefinition> = request
            .tools
            .iter()
            .map(|t| OpenAIToolDefinition {
                tool_type: "function".to_string(),
                function: OpenAIFunctionDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect();

        // Strip provider prefix (e.g. "zai/glm-5" → "glm-5")
        let model_name = request.model.split_once('/').map_or(request.model.as_str(), |(_,m)| m);

        let api_request = OpenAIRequest {
            model: model_name.to_string(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop,
            stream: false,
            tools,
        };

        let url = format!("{}{}", self.base_url, self.completions_path);

        // Serialize request and inject extra body parameters
        let mut body = serde_json::to_value(&api_request)?;
        if let Some(obj) = body.as_object_mut() {
            for (key, value) in &self.extra_body {
                obj.insert(key.clone(), value.clone());
            }
        }

        let mut req = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        // Add any extra headers
        for (key, value) in &self.extra_headers {
            req = req.header(key, value);
        }

        let response = req.json(&body).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("{} API error: {}", self.provider_id, error_text);
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

        // Use reasoning_content as fallback when content is empty/null
        // (reasoning models like GLM-5 put all output in reasoning_content)
        let content_str = choice.message.content.as_deref().unwrap_or("");
        let response_content = if content_str.is_empty() {
            choice.message.reasoning_content.clone().unwrap_or_default()
        } else {
            content_str.to_string()
        };

        // Parse tool calls from response
        let tool_calls: Vec<ToolCallResponse> = choice
            .message
            .tool_calls
            .iter()
            .filter_map(|tc| {
                let args: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
                Some(ToolCallResponse {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    arguments: args,
                })
            })
            .collect();

        Ok(CompletionResponse {
            content: response_content,
            model: api_response.model,
            finish_reason: choice
                .finish_reason
                .as_ref()
                .map(|r| Self::convert_finish_reason(r))
                .unwrap_or(FinishReason::Stop),
            tool_calls,
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
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
        let messages: Vec<serde_json::Value> = request
            .messages
            .into_iter()
            .map(|msg| Self::message_to_json(&msg))
            .collect();

        // Convert tool definitions to OpenAI format
        let tools: Vec<OpenAIToolDefinition> = request
            .tools
            .iter()
            .map(|t| OpenAIToolDefinition {
                tool_type: "function".to_string(),
                function: OpenAIFunctionDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect();

        // Strip provider prefix (e.g. "zai/glm-5" → "glm-5")
        let model_name = request.model.split_once('/').map_or(request.model.as_str(), |(_,m)| m);

        let api_request = OpenAIRequest {
            model: model_name.to_string(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop,
            stream: true,
            tools,
        };

        eprintln!("[{}] complete_stream: model={}, tools={}, messages={}",
            self.provider_id, api_request.model, api_request.tools.len(), api_request.messages.len());
        if !api_request.tools.is_empty() {
            let tool_names: Vec<&str> = api_request.tools.iter().map(|t| t.function.name.as_str()).collect();
            eprintln!("[{}] tools: {:?}", self.provider_id, tool_names);
        }

        let url = format!("{}{}", self.base_url, self.completions_path);

        // Serialize request and inject extra body parameters
        let mut body = serde_json::to_value(&api_request)?;
        if let Some(obj) = body.as_object_mut() {
            for (key, value) in &self.extra_body {
                obj.insert(key.clone(), value.clone());
            }
        }

        let mut req = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        // Add any extra headers
        for (key, value) in &self.extra_headers {
            req = req.header(key, value);
        }

        let response = req.json(&body).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("{} API error: {}", self.provider_id, error_text);
        }

        let provider_id = self.provider_id.clone();
        let byte_stream = response.bytes_stream();

        // State for accumulating streamed tool calls.
        // Each tool call arrives incrementally: the first chunk for an index has
        // id+name, subsequent chunks append argument fragments. We accumulate by
        // index and, on finish, emit one native StreamEvent::ToolCall per call.
        #[derive(Default)]
        struct ToolCallAccumulator {
            calls: Vec<(String, String, String)>, // (id, name, arguments_json)
        }

        // Drain accumulated calls into typed ToolCall events. Arguments are
        // parsed to JSON (empty/invalid → {}). Skips empty (id+name blank) slots.
        //
        // An id MUST be non-empty and unique: it becomes the assistant
        // message's tool_calls[].id AND the matching tool message's
        // tool_call_id on the next turn, and Z.AI/GLM reject a tool round whose
        // ids are blank or duplicated. Some providers stream the id only in the
        // first delta for an index; if we somehow accumulated a named call with
        // no id, synthesize a stable per-index id so the round stays valid.
        fn flush_tool_calls(acc: &mut ToolCallAccumulator, pid: &str) -> Vec<anyhow::Result<StreamEvent>> {
            let mut out = Vec::new();
            for (idx, (mut id, name, args)) in acc.calls.drain(..).enumerate() {
                if id.is_empty() && name.is_empty() {
                    continue;
                }
                if id.is_empty() {
                    id = format!("call_{pid}_{idx}");
                    eprintln!("[{}] tool_call had empty id; synthesized {}", pid, id);
                }
                eprintln!("[{}] emitting native tool_call: id={}, name={}", pid, id, name);
                let arguments: serde_json::Value =
                    serde_json::from_str(&args).unwrap_or_else(|_| serde_json::json!({}));
                out.push(Ok(StreamEvent::ToolCall(ToolCallResponse { id, name, arguments })));
            }
            out
        }

        let stream = byte_stream
            .map(|result| result.map_err(|e| anyhow::anyhow!("Stream error: {}", e)))
            .scan((String::new(), ToolCallAccumulator::default(), provider_id, false, None::<FinishReason>), |(buffer, tool_acc, pid, first_logged, finish), result| {
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
                                        eprintln!("[{}] stream [DONE], accumulated tool_calls: {}", pid, tool_acc.calls.len());
                                        // Emit any tool calls not already flushed at finish_reason.
                                        chunks.extend(flush_tool_calls(tool_acc, pid));
                                        chunks.push(Ok(StreamEvent::Done(finish.take())));
                                        continue;
                                    }
                                    // Log first SSE data line for debugging
                                    if !*first_logged {
                                        eprintln!("[{}] first SSE data: {}...", pid, &data[..data.len().min(200)]);
                                        *first_logged = true;
                                    }
                                    match serde_json::from_str::<StreamChunk>(data) {
                                        Err(e) => {
                                            eprintln!("[{}] SSE parse error: {} for data: {}...", pid, e, &data[..data.len().min(200)]);
                                        }
                                        Ok(chunk) => {
                                            if let Some(choice) = chunk.choices.first() {
                                                // Handle text content
                                                if let Some(content) = &choice.delta.content {
                                                    if !content.is_empty() {
                                                        chunks.push(Ok(StreamEvent::TextDelta(content.clone())));
                                                    }
                                                }
                                                // Deliberately DO NOT stream `reasoning_content` as
                                                // reply text. It is the model's private chain-of-thought
                                                // (GLM-5, DeepSeek-R1). Streaming it leaked the reasoning
                                                // of every tool-call round ("The user is asking…",
                                                // "Let me search…") plus serialized tool calls into the
                                                // visible reply. The real answer arrives via
                                                // `delta.content`; reasoning is dropped from the reply.
                                                // Accumulate tool calls
                                                if !choice.delta.tool_calls.is_empty() {
                                                    eprintln!("[{}] stream chunk has {} tool_calls", pid, choice.delta.tool_calls.len());
                                                }
                                                if let Some(ref fr) = choice.finish_reason {
                                                    eprintln!("[{}] stream finish_reason: {}", pid, fr);
                                                }
                                                for tc in &choice.delta.tool_calls {
                                                    let idx = tc.index;
                                                    while tool_acc.calls.len() <= idx {
                                                        tool_acc.calls.push((String::new(), String::new(), String::new()));
                                                    }
                                                    if let Some(id) = &tc.id {
                                                        tool_acc.calls[idx].0 = id.clone();
                                                    }
                                                    if let Some(ref f) = tc.function {
                                                        if let Some(ref name) = f.name {
                                                            tool_acc.calls[idx].1 = name.clone();
                                                        }
                                                        if let Some(ref args) = f.arguments {
                                                            tool_acc.calls[idx].2.push_str(args);
                                                        }
                                                    }
                                                }
                                                // On a terminal finish_reason, flush accumulated
                                                // tool calls and record the reason. The single
                                                // Done event is emitted at the SSE [DONE] sentinel
                                                // (always last) so we never double-emit Done.
                                                if let Some(fr) = choice.finish_reason.as_deref() {
                                                    chunks.extend(flush_tool_calls(tool_acc, pid));
                                                    *finish = Some(Self::convert_finish_reason(fr));
                                                }
                                            }
                                        }
                                    } // match
                                } // if let Some(data)
                            } // for line
                        } // while
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

// =============================================================================
// Model Catalogs
// =============================================================================

/// Z.AI (Zhipu GLM) models
pub fn zai_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "glm-5".to_string(),
            name: "GLM-5".to_string(),
            context_length: 200000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "glm-4.7".to_string(),
            name: "GLM-4.7".to_string(),
            context_length: 200000,
            capabilities: vec![
                "chat".to_string(),
                "tools".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "glm-4.6".to_string(),
            name: "GLM-4.6".to_string(),
            context_length: 200000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "glm-4-plus".to_string(),
            name: "GLM-4 Plus".to_string(),
            context_length: 128000,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
        },
    ]
}

/// Groq models
pub fn groq_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "llama-4-maverick".to_string(),
            name: "Llama 4 Maverick 17B".to_string(),
            context_length: 128000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        },
        ModelInfo {
            id: "llama-4-scout".to_string(),
            name: "Llama 4 Scout 17B".to_string(),
            context_length: 128000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
                "code".to_string(),
                "reasoning".to_string(),
            ],
        },
        ModelInfo {
            id: "llama-3.3-70b-versatile".to_string(),
            name: "Llama 3.3 70B".to_string(),
            context_length: 131072,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
        },
        ModelInfo {
            id: "mixtral-8x7b-32768".to_string(),
            name: "Mixtral 8x7B".to_string(),
            context_length: 32768,
            capabilities: vec!["chat".to_string()],
        },
        ModelInfo {
            id: "gemma2-9b-it".to_string(),
            name: "Gemma 2 9B".to_string(),
            context_length: 8192,
            capabilities: vec!["chat".to_string()],
        },
    ]
}

/// DeepSeek models
pub fn deepseek_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "deepseek-v3.2".to_string(),
            name: "DeepSeek V3.2".to_string(),
            context_length: 256000,
            capabilities: vec![
                "chat".to_string(),
                "tools".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "deepseek-v3.2-speciale".to_string(),
            name: "DeepSeek V3.2 Speciale".to_string(),
            context_length: 256000,
            capabilities: vec![
                "chat".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "deepseek-chat".to_string(),
            name: "DeepSeek Chat".to_string(),
            context_length: 128000,
            capabilities: vec!["chat".to_string()],
        },
        ModelInfo {
            id: "deepseek-reasoner".to_string(),
            name: "DeepSeek Reasoner".to_string(),
            context_length: 128000,
            capabilities: vec!["chat".to_string(), "reasoning".to_string()],
        },
    ]
}

/// Mistral models
pub fn mistral_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "mistral-large-3".to_string(),
            name: "Mistral Large 3".to_string(),
            context_length: 256000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        },
        ModelInfo {
            id: "magistral-medium".to_string(),
            name: "Magistral Medium".to_string(),
            context_length: 128000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "reasoning".to_string(),
            ],
        },
        ModelInfo {
            id: "magistral-small".to_string(),
            name: "Magistral Small".to_string(),
            context_length: 128000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "reasoning".to_string(),
            ],
        },
        ModelInfo {
            id: "mistral-large-latest".to_string(),
            name: "Mistral Large (Latest)".to_string(),
            context_length: 128000,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
        },
        ModelInfo {
            id: "codestral-latest".to_string(),
            name: "Codestral".to_string(),
            context_length: 256000,
            capabilities: vec!["chat".to_string(), "code".to_string()],
        },
    ]
}

/// Venice.ai models
pub fn venice_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "qwen-3-coder-480b".to_string(),
            name: "Qwen 3 Coder 480B".to_string(),
            context_length: 262000,
            capabilities: vec!["chat".to_string(), "code".to_string(), "tools".to_string()],
        },
        ModelInfo {
            id: "qwen-3-next-80b".to_string(),
            name: "Qwen 3 Next 80B".to_string(),
            context_length: 262000,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
        },
        ModelInfo {
            id: "llama-3.3-70b".to_string(),
            name: "Llama 3.3 70B".to_string(),
            context_length: 128000,
            capabilities: vec!["chat".to_string(), "code".to_string(), "reasoning".to_string()],
        },
        ModelInfo {
            id: "qwen3-235b".to_string(),
            name: "Qwen 3 235B".to_string(),
            context_length: 262000,
            capabilities: vec!["chat".to_string(), "code".to_string()],
        },
    ]
}

/// Qwen (DashScope) models
pub fn qwen_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "qwen3-235b-a22b-instruct".to_string(),
            name: "Qwen 3 235B A22B".to_string(),
            context_length: 256000,
            capabilities: vec![
                "chat".to_string(),
                "tools".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "qwen3-30b-a3b-instruct".to_string(),
            name: "Qwen 3 30B A3B".to_string(),
            context_length: 256000,
            capabilities: vec![
                "chat".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "qwen3-32b".to_string(),
            name: "Qwen 3 32B".to_string(),
            context_length: 256000,
            capabilities: vec!["chat".to_string(), "code".to_string()],
        },
        ModelInfo {
            id: "qwen3-14b".to_string(),
            name: "Qwen 3 14B".to_string(),
            context_length: 256000,
            capabilities: vec!["chat".to_string(), "code".to_string()],
        },
        ModelInfo {
            id: "qwen-max".to_string(),
            name: "Qwen Max".to_string(),
            context_length: 1000000,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
        },
    ]
}

/// Google Gemini models
pub fn gemini_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "gemini-3-pro".to_string(),
            name: "Gemini 3 Pro".to_string(),
            context_length: 1000000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "gemini-3-flash".to_string(),
            name: "Gemini 3 Flash".to_string(),
            context_length: 1000000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        },
        ModelInfo {
            id: "gemini-2.5-pro".to_string(),
            name: "Gemini 2.5 Pro".to_string(),
            context_length: 1000000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
                "reasoning".to_string(),
                "code".to_string(),
            ],
        },
        ModelInfo {
            id: "gemini-2.5-flash".to_string(),
            name: "Gemini 2.5 Flash".to_string(),
            context_length: 1048576,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        },
        ModelInfo {
            id: "gemini-2.5-flash-lite".to_string(),
            name: "Gemini 2.5 Flash-Lite".to_string(),
            context_length: 1000000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_provider(id: &str) -> OpenAICompatibleProvider {
        OpenAICompatibleProvider::new(OpenAICompatibleConfig {
            provider_id: id.to_string(),
            api_key: "test-key".to_string(),
            base_url: "https://example.com".to_string(),
            models: vec![ModelInfo {
                id: "test-model".to_string(),
                name: "Test Model".to_string(),
                context_length: 4096,
                capabilities: vec!["chat".to_string()],
            }],
            completions_path: None,
            extra_headers: vec![],
            extra_body: vec![],
        })
    }

    #[test]
    fn test_provider_id() {
        let p = test_provider("groq");
        assert_eq!(p.id(), "groq");
    }

    #[test]
    fn test_models_list() {
        let p = test_provider("groq");
        assert_eq!(p.models().len(), 1);
    }

    #[test]
    fn test_convert_role() {
        assert_eq!(OpenAICompatibleProvider::convert_role(Role::System), "system");
        assert_eq!(OpenAICompatibleProvider::convert_role(Role::User), "user");
        assert_eq!(
            OpenAICompatibleProvider::convert_role(Role::Assistant),
            "assistant"
        );
    }

    #[test]
    fn test_convert_finish_reason() {
        assert_eq!(
            OpenAICompatibleProvider::convert_finish_reason("stop"),
            FinishReason::Stop
        );
        assert_eq!(
            OpenAICompatibleProvider::convert_finish_reason("length"),
            FinishReason::Length
        );
        assert_eq!(
            OpenAICompatibleProvider::convert_finish_reason("tool_calls"),
            FinishReason::ToolUse
        );
        assert_eq!(
            OpenAICompatibleProvider::convert_finish_reason("content_filter"),
            FinishReason::ContentFilter
        );
    }

    #[test]
    fn test_all_model_catalogs() {
        assert!(!zai_models().is_empty());
        assert!(!groq_models().is_empty());
        assert!(!deepseek_models().is_empty());
        assert!(!mistral_models().is_empty());
        assert!(!venice_models().is_empty());
        assert!(!qwen_models().is_empty());
        assert!(!gemini_models().is_empty());
    }

    #[test]
    fn test_model_catalog_details() {
        let zai = zai_models();
        assert!(zai.iter().any(|m| m.id == "glm-5"));
        assert!(zai.iter().any(|m| m.context_length == 200000)); // GLM-5

        let groq = groq_models();
        assert!(groq.iter().any(|m| m.id == "llama-4-maverick"));

        let deepseek = deepseek_models();
        assert!(deepseek
            .iter()
            .any(|m| m.capabilities.contains(&"reasoning".to_string())));
    }

    #[test]
    fn test_url_construction() {
        let p1 = OpenAICompatibleProvider::new(OpenAICompatibleConfig {
            provider_id: "test".to_string(),
            api_key: "key".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            models: vec![],
            completions_path: Some("/chat/completions".to_string()),
            extra_headers: vec![],
            extra_body: vec![],
        });
        assert_eq!(p1.base_url, "https://api.example.com/v1");
        assert_eq!(p1.completions_path, "/chat/completions");

        // Test trailing slash removal
        let p2 = OpenAICompatibleProvider::new(OpenAICompatibleConfig {
            provider_id: "test".to_string(),
            api_key: "key".to_string(),
            base_url: "https://api.example.com/v1/".to_string(),
            models: vec![],
            completions_path: None,
            extra_headers: vec![],
            extra_body: vec![],
        });
        assert_eq!(p2.base_url, "https://api.example.com/v1");
    }
}
