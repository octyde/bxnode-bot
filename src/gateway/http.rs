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

use super::{AppState, ApiChatRequest, UiAssets};
use crate::providers::{CompletionRequest, Message, Role};
use crate::skills::{SkillInfo, SkillSyncer, SyncReport};

/// Simple health check endpoint (for load balancers)
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Detailed health/status endpoint
pub async fn health_detailed(State(state): State<AppState>) -> impl IntoResponse {
    let channels = state.channels.all_status().await;
    let providers = state.providers.provider_ids();
    let cron_jobs = state.cron.list_jobs().await;
    let memory_stats = {
        let store = state.memory.read().await;
        serde_json::json!({
            "enabled": store.len() > 0 || store.file_path().is_some(),
            "record_count": store.len(),
            "persistent": store.file_path().is_some(),
        })
    };

    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_info": {
            "started": chrono::Utc::now().to_rfc3339(),
        },
        "subsystems": {
            "providers": {
                "count": providers.len(),
                "ids": providers,
            },
            "channels": channels.iter().map(|c| serde_json::json!({
                "id": c.id,
                "name": c.name,
                "connected": c.connected,
                "error": c.error,
            })).collect::<Vec<_>>(),
            "cron": {
                "enabled": true,
                "job_count": cron_jobs.len(),
                "jobs": cron_jobs.iter().map(|j| serde_json::json!({
                    "id": j.id,
                    "schedule": j.schedule,
                    "enabled": j.enabled,
                    "last_run": j.last_run,
                })).collect::<Vec<_>>(),
            },
            "memory": memory_stats,
        }
    }))
}

/// Stats/metrics endpoint
pub async fn stats(State(state): State<AppState>) -> impl IntoResponse {
    let channel_count = state.channels.channel_count().await;
    let provider_count = state.providers.provider_count();
    let model_count = state.providers.all_models_prefixed().len();
    let cron_job_count = state.cron.list_jobs().await.len();
    let memory_count = {
        let store = state.memory.read().await;
        store.len()
    };

    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "providers": provider_count,
        "models": model_count,
        "channels": channel_count,
        "cron_jobs": cron_job_count,
        "memories": memory_count,
    }))
}

/// Session stats endpoint — active sessions, usage, and token stats
pub async fn session_stats(State(state): State<AppState>) -> impl IntoResponse {
    let active_count = {
        let active = state.active_sessions.read().await;
        active.count()
    };
    let api_context_count = {
        let ctxs = state.api_contexts.read().await;
        ctxs.len()
    };

    let tokens_in = state.usage_stats.tokens_in.load(std::sync::atomic::Ordering::Relaxed);
    let tokens_out = state.usage_stats.tokens_out.load(std::sync::atomic::Ordering::Relaxed);
    let compactions = state.usage_stats.compactions.load(std::sync::atomic::Ordering::Relaxed);

    Json(serde_json::json!({
        "active_sessions": active_count,
        "api_contexts": api_context_count,
        "tokens_in": tokens_in,
        "tokens_out": tokens_out,
        "compactions": compactions,
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
        .map(|m| {
            let role = match m.role.as_str() {
                "system" => Role::System,
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => Role::User,
            };
            Message::text(role, m.content.clone())
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
        tools: vec![],
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

// ============================================================================
// Skills API Handlers
// ============================================================================

/// Skill detail response including instructions
#[derive(Debug, Serialize)]
pub struct SkillDetail {
    #[serde(flatten)]
    pub info: SkillInfo,
    pub instructions: Option<String>,
}

/// List all skills
pub async fn list_skills(State(state): State<AppState>) -> Response {
    let Some(ref skills) = state.skills else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Skills system is not enabled"
            })),
        )
            .into_response();
    };

    let registry = skills.read().await;
    let skill_list = registry.list_info();

    Json(skill_list).into_response()
}

/// Get skill details by name
pub async fn get_skill(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Response {
    let Some(ref skills) = state.skills else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Skills system is not enabled"
            })),
        )
            .into_response();
    };

    let registry = skills.read().await;

    match registry.get(&name) {
        Some(skill_ref) => {
            let mut info = SkillInfo::from(skill_ref);
            info.is_active = registry.is_active(&name);

            let detail = SkillDetail {
                info,
                instructions: skill_ref.instructions().map(|s| s.to_string()),
            };
            Json(detail).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Skill '{}' not found", name)
            })),
        )
            .into_response(),
    }
}

/// Enable a skill
pub async fn enable_skill(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Response {
    let Some(ref skills) = state.skills else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Skills system is not enabled"
            })),
        )
            .into_response();
    };

    let mut registry = skills.write().await;

    match registry.enable(&name) {
        Ok(()) => Json(serde_json::json!({
            "success": true,
            "message": format!("Skill '{}' enabled", name)
        }))
        .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Disable a skill
pub async fn disable_skill(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Response {
    let Some(ref skills) = state.skills else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Skills system is not enabled"
            })),
        )
            .into_response();
    };

    let mut registry = skills.write().await;
    registry.disable(&name);

    Json(serde_json::json!({
        "success": true,
        "message": format!("Skill '{}' disabled", name)
    }))
    .into_response()
}

/// Sync report response
#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub success: bool,
    pub synced: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<(String, String)>,
    pub total: usize,
}

impl From<SyncReport> for SyncResponse {
    fn from(report: SyncReport) -> Self {
        let total = report.total();
        Self {
            success: report.is_success(),
            synced: report.synced,
            skipped: report.skipped,
            errors: report.errors,
            total,
        }
    }
}

/// Sync skills from configured sources
pub async fn sync_skills(State(state): State<AppState>) -> Response {
    let Some(ref skills) = state.skills else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Skills system is not enabled"
            })),
        )
            .into_response();
    };

    // Get the skills directory from registry
    let registry = skills.read().await;
    let directories = registry.directories();

    if directories.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "No skill directories configured"
            })),
        )
            .into_response();
    }

    // Use the first directory as the target for syncing
    let target_dir = directories[0].clone();
    drop(registry); // Release the read lock

    let syncer = SkillSyncer::new(target_dir);

    match syncer.sync_from_awesome_list(false).await {
        Ok(report) => {
            // Rescan skills after sync
            let mut registry = skills.write().await;
            if let Err(e) = registry.scan_all() {
                tracing::warn!("Failed to rescan skills after sync: {}", e);
            }

            Json(SyncResponse::from(report)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Get skills configuration summary
pub async fn skills_config(State(state): State<AppState>) -> Response {
    let Some(ref skills) = state.skills else {
        return Json(serde_json::json!({
            "enabled": false
        }))
        .into_response();
    };

    let registry = skills.read().await;
    let directories: Vec<String> = registry
        .directories()
        .iter()
        .map(|p| p.display().to_string())
        .collect();

    Json(serde_json::json!({
        "enabled": true,
        "total_skills": registry.count(),
        "active_skills": registry.active_count(),
        "directories": directories,
    }))
    .into_response()
}

// ============================================================================
// Chat API (full gateway pipeline: slash commands + agent)
// ============================================================================

/// POST /api/chat — send a message through the full gateway pipeline.
///
/// Handles slash commands locally and dispatches everything else to the agent.
pub async fn chat(
    State(state): State<AppState>,
    Json(request): Json<ApiChatRequest>,
) -> Response {
    tracing::info!("[api/chat] message={:?}, context_key={}", &request.message[..request.message.len().min(80)], request.context_key);

    match super::process_api_message(&state, &request).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => {
            tracing::error!("[api/chat] error: {}", e);
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
