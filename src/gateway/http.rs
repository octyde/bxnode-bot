//! HTTP handlers for the gateway

use std::convert::Infallible;

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};

use super::{AppState, UiAssets};
use crate::providers::{CompletionRequest, Message, Role};

/// Health check endpoint
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// OpenAI-compatible chat completions request
#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// OpenAI-compatible chat completions response
#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Handle chat completions request
pub async fn chat_completions(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Response {
    tracing::info!("Chat completion request for model: {}", request.model);

    // Find the provider for this model
    let provider = match state.providers.get_for_model(&request.model) {
        Some(p) => p,
        None => {
            tracing::warn!("No provider found for model: {}", request.model);
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "message": format!("Model '{}' not found. No provider configured for this model.", request.model),
                        "type": "invalid_request_error",
                        "code": "model_not_found"
                    }
                })),
            )
                .into_response();
        }
    };

    // Extract the actual model name (remove provider prefix if present)
    let model_name = state.providers.extract_model_name(&request.model);

    // Convert messages to provider format
    let messages: Vec<Message> = request
        .messages
        .iter()
        .map(|m| Message {
            role: match m.role.as_str() {
                "system" => Role::System,
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => Role::User,
            },
            content: m.content.clone(),
        })
        .collect();

    // Build completion request
    let completion_request = CompletionRequest {
        model: model_name.clone(),
        messages,
        temperature: request.temperature,
        max_tokens: request.max_tokens,
        stop: vec![],
        stream: request.stream,
    };

    if request.stream {
        // Streaming response
        match provider.complete_stream(completion_request).await {
            Ok(stream) => {
                let sse_stream = create_sse_stream(stream, request.model.clone());
                Sse::new(sse_stream)
                    .keep_alive(KeepAlive::default())
                    .into_response()
            }
            Err(e) => {
                tracing::error!("Streaming completion error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {
                            "message": e.to_string(),
                            "type": "api_error"
                        }
                    })),
                )
                    .into_response()
            }
        }
    } else {
        // Non-streaming response
        match provider.complete(completion_request).await {
            Ok(completion) => {
                let response = ChatCompletionResponse {
                    id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                    object: "chat.completion".to_string(),
                    created: chrono::Utc::now().timestamp(),
                    model: request.model,
                    choices: vec![ChatChoice {
                        index: 0,
                        message: ChatMessage {
                            role: "assistant".to_string(),
                            content: completion.content,
                        },
                        finish_reason: match completion.finish_reason {
                            crate::providers::FinishReason::Stop => "stop".to_string(),
                            crate::providers::FinishReason::Length => "length".to_string(),
                            crate::providers::FinishReason::ToolUse => "tool_calls".to_string(),
                            crate::providers::FinishReason::ContentFilter => {
                                "content_filter".to_string()
                            }
                        },
                    }],
                    usage: Usage {
                        prompt_tokens: completion.usage.prompt_tokens,
                        completion_tokens: completion.usage.completion_tokens,
                        total_tokens: completion.usage.total_tokens,
                    },
                };
                Json(response).into_response()
            }
            Err(e) => {
                tracing::error!("Completion error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {
                            "message": e.to_string(),
                            "type": "api_error"
                        }
                    })),
                )
                    .into_response()
            }
        }
    }
}

/// Create SSE stream from provider stream
fn create_sse_stream(
    stream: futures_util::stream::BoxStream<'static, anyhow::Result<String>>,
    model: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let created = chrono::Utc::now().timestamp();

    stream
        .map(move |result| {
            let event = match result {
                Ok(content) => {
                    let chunk = StreamChunk {
                        id: id.clone(),
                        object: "chat.completion.chunk".to_string(),
                        created,
                        model: model.clone(),
                        choices: vec![StreamChoice {
                            index: 0,
                            delta: Delta {
                                role: None,
                                content: Some(content),
                            },
                            finish_reason: None,
                        }],
                    };
                    Event::default().data(serde_json::to_string(&chunk).unwrap_or_default())
                }
                Err(e) => {
                    tracing::error!("Stream error: {}", e);
                    Event::default().data(format!(
                        "{{\"error\": {{\"message\": \"{}\"}}}}",
                        e.to_string().replace('"', "\\\"")
                    ))
                }
            };
            Ok(event)
        })
        .chain(futures_util::stream::once(async move {
            // Send final chunk with finish_reason
            let final_chunk = StreamChunk {
                id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                object: "chat.completion.chunk".to_string(),
                created: chrono::Utc::now().timestamp(),
                model: String::new(),
                choices: vec![StreamChoice {
                    index: 0,
                    delta: Delta {
                        role: None,
                        content: None,
                    },
                    finish_reason: Some("stop".to_string()),
                }],
            };
            Ok(Event::default().data(serde_json::to_string(&final_chunk).unwrap_or_default()))
        }))
        .chain(futures_util::stream::once(async {
            Ok(Event::default().data("[DONE]"))
        }))
}

/// Streaming chunk response
#[derive(Debug, Serialize)]
struct StreamChunk {
    id: String,
    object: String,
    created: i64,
    model: String,
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Serialize)]
struct StreamChoice {
    index: u32,
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

/// Model information
#[derive(Debug, Serialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<Model>,
}

/// List available models
pub async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    let models: Vec<Model> = state
        .providers
        .all_models_prefixed()
        .into_iter()
        .map(|pm| Model {
            id: pm.id,
            object: "model".to_string(),
            created: 0,
            owned_by: pm.provider,
        })
        .collect();

    let response = ModelsResponse {
        object: "list".to_string(),
        data: models,
    };

    Json(response)
}

/// Serve embedded UI assets
pub async fn serve_ui(request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match UiAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => {
            // Try index.html for SPA routing
            match UiAssets::get("index.html") {
                Some(content) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(content.data.to_vec()))
                    .unwrap(),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Not Found"))
                    .unwrap(),
            }
        }
    }
}
