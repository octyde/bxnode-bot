//! WebSocket handler for RPC communication

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use super::{AppState, BroadcastEvent};

/// WebSocket upgrade handler
pub async fn handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle WebSocket connection with bidirectional communication
async fn handle_socket(socket: WebSocket, state: AppState) {
    tracing::info!("New WebSocket connection");

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Send welcome message
    let welcome = RpcMessage::Event {
        event: "connected".to_string(),
        payload: serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "server": "bxnode-bot"
        }),
    };

    if let Err(e) = ws_sender
        .send(Message::Text(serde_json::to_string(&welcome).unwrap().into()))
        .await
    {
        tracing::error!("Failed to send welcome: {}", e);
        return;
    }

    // Subscribe to broadcast events
    let mut broadcast_rx = state.event_bus.subscribe();

    // Server-side keepalive ping interval (prevents idle timeout disconnects)
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    ping_interval.tick().await; // consume first immediate tick

    loop {
        tokio::select! {
            // Handle incoming client messages
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_message(&mut ws_sender, &text, &state).await {
                            tracing::error!("Error handling message: {}", e);
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        if let Ok(text) = String::from_utf8(data.to_vec()) {
                            if let Err(e) = handle_message(&mut ws_sender, &text, &state).await {
                                tracing::error!("Error handling binary message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("WebSocket closed by client");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            // Forward broadcast events to this client
            event = broadcast_rx.recv() => {
                match event {
                    Ok(broadcast_event) => {
                        let rpc_event = RpcMessage::Event {
                            event: broadcast_event.event_name().to_string(),
                            payload: serde_json::to_value(&broadcast_event)
                                .unwrap_or(serde_json::Value::Null),
                        };
                        if let Err(e) = ws_sender
                            .send(Message::Text(serde_json::to_string(&rpc_event).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send broadcast event: {}", e);
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client lagged, missed {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            // Server-side keepalive ping
            _ = ping_interval.tick() => {
                if let Err(e) = ws_sender.send(Message::Ping(vec![].into())).await {
                    tracing::debug!("Ping failed, closing connection: {}", e);
                    break;
                }
            }
        }
    }

    tracing::info!("WebSocket connection closed");
}

/// RPC message types
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RpcMessage {
    #[serde(rename = "request")]
    Request {
        id: String,
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },

    #[serde(rename = "response")]
    Response {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RpcError>,
    },

    #[serde(rename = "event")]
    Event {
        event: String,
        payload: serde_json::Value,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// Handle incoming RPC message
async fn handle_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    text: &str,
    state: &AppState,
) -> anyhow::Result<()> {
    let message: RpcMessage = serde_json::from_str(text)?;

    match message {
        RpcMessage::Request { id, method, params } => {
            tracing::debug!("RPC request: {} (id: {})", method, id);

            let result = handle_rpc_method(&method, params, state).await;

            let response = match result {
                Ok(value) => RpcMessage::Response {
                    id,
                    result: Some(value),
                    error: None,
                },
                Err(e) => RpcMessage::Response {
                    id,
                    result: None,
                    error: Some(RpcError {
                        code: -1,
                        message: e.to_string(),
                    }),
                },
            };

            sender
                .send(Message::Text(serde_json::to_string(&response)?.into()))
                .await?;
        }
        _ => {
            tracing::warn!("Unexpected message type from client");
        }
    }

    Ok(())
}

/// Handle RPC method calls
async fn handle_rpc_method(
    method: &str,
    params: serde_json::Value,
    state: &AppState,
) -> anyhow::Result<serde_json::Value> {
    match method {
        "health" => Ok(serde_json::json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION")
        })),

        "server.restart" => {
            tracing::info!("Server restart requested via RPC");
            // Signal shutdown with restart flag (true = restart)
            let _ = state.shutdown_tx.send(true);
            Ok(serde_json::json!({ "restarting": true }))
        }

        "models" => {
            let models: Vec<serde_json::Value> = state
                .providers
                .all_models_prefixed()
                .into_iter()
                .map(|pm| {
                    serde_json::json!({
                        "id": pm.id,
                        "provider": pm.provider,
                        "name": pm.model.name,
                        "context_length": pm.model.context_length
                    })
                })
                .collect();
            Ok(serde_json::json!({ "models": models }))
        }

        "providers" => {
            let providers = state.providers.provider_ids();
            Ok(serde_json::json!({ "providers": providers }))
        }

        "channels.status" => {
            let statuses = state.channels.all_status().await;
            let channels: Vec<serde_json::Value> = statuses
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "name": s.name,
                        "connected": s.connected,
                        "error": s.error
                    })
                })
                .collect();
            Ok(serde_json::json!({ "channels": channels }))
        }

        // ── Session management ────────────────────────────────────

        "sessions.list" => {
            let channel = params.get("channel").and_then(|v| v.as_str());
            let chat_id = params.get("chat_id").and_then(|v| v.as_str());

            let sessions = if let (Some(ch), Some(cid)) = (channel, chat_id) {
                state.sessions.list_for_chat(ch, cid).await?
            } else {
                state.sessions.list().await?
            };

            let mut sorted = sessions;
            sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            let items: Vec<serde_json::Value> = sorted
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "channel": s.channel,
                        "chat_id": s.chat_id,
                        "project": s.project,
                        "title": s.title,
                        "created_at": s.created_at.to_rfc3339(),
                        "updated_at": s.updated_at.to_rfc3339(),
                    })
                })
                .collect();
            Ok(serde_json::json!({ "sessions": items }))
        }

        "sessions.transcript" => {
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'session_id' parameter"))?;

            // We need the project to load transcript — find the session first
            let session = state.sessions.load(session_id).await?
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

            let entries = state.sessions.load_transcript(&session.project, session_id).await?;

            let items: Vec<serde_json::Value> = entries
                .into_iter()
                .map(|e| serde_json::to_value(&e).unwrap_or(serde_json::Value::Null))
                .collect();
            Ok(serde_json::json!({ "entries": items }))
        }

        // ── Project management ───────────────────────────────────

        "projects.list" => {
            let store = state.project_store.read().await;
            let project_list = store.list().await.unwrap_or_default();

            let mut projects: Vec<serde_json::Value> = Vec::new();
            for proj in &project_list {
                let count = state.sessions.count_sessions_in_project(&proj.name).await.unwrap_or(0);
                projects.push(serde_json::json!({
                    "name": proj.name,
                    "workspace_dir": proj.workspace_dir.display().to_string(),
                    "model": proj.model,
                    "coding_tools_enabled": proj.coding_tools_enabled,
                    "shell_enabled": proj.shell_enabled,
                    "created_at": proj.created_at.to_rfc3339(),
                    "session_count": count,
                }));
            }
            Ok(serde_json::json!({ "projects": projects }))
        }

        "projects.get" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;

            let store = state.project_store.read().await;
            match store.load(name).await? {
                Some(proj) => {
                    let count = state.sessions.count_sessions_in_project(name).await.unwrap_or(0);
                    Ok(serde_json::json!({
                        "name": proj.name,
                        "workspace_dir": proj.workspace_dir.display().to_string(),
                        "description": proj.description,
                        "model": proj.model,
                        "system_prompt": proj.system_prompt,
                        "coding_tools_enabled": proj.coding_tools_enabled,
                        "shell_enabled": proj.shell_enabled,
                        "created_at": proj.created_at.to_rfc3339(),
                        "session_count": count,
                    }))
                }
                None => Err(anyhow::anyhow!("Project not found: {}", name)),
            }
        }

        "projects.create" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;

            if !crate::project::validate_project_name(name) {
                return Err(anyhow::anyhow!("Invalid project name"));
            }

            let workspace_dir = {
                let store = state.project_store.read().await;
                if store.exists(name).await {
                    return Err(anyhow::anyhow!("Project '{}' already exists", name));
                }
                store.resolve_workspace_dir(name)
            };

            let project = crate::project::Project {
                name: name.to_string(),
                workspace_dir: workspace_dir.clone(),
                description: params.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()),
                model: params.get("model").and_then(|v| v.as_str()).map(|s| s.to_string()),
                system_prompt: params.get("system_prompt").and_then(|v| v.as_str()).map(|s| s.to_string()),
                coding_tools_enabled: params.get("coding_tools_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                shell_enabled: params.get("shell_enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                metadata: serde_json::json!({}),
                created_at: chrono::Utc::now(),
            };

            {
                let store = state.project_store.write().await;
                store.create(&project).await?;
            }

            // Create workspace directory
            tokio::fs::create_dir_all(&workspace_dir).await?;

            // Also create in sessions
            let _ = state.sessions.create_project(name).await;

            Ok(serde_json::json!({
                "name": name,
                "workspace_dir": workspace_dir.display().to_string(),
            }))
        }

        "projects.update" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;

            let store = state.project_store.write().await;
            let mut project = store.load(name).await?
                .ok_or_else(|| anyhow::anyhow!("Project not found: {}", name))?;

            // Apply updates
            if let Some(v) = params.get("model") {
                project.model = v.as_str().map(|s| s.to_string());
            }
            if let Some(v) = params.get("system_prompt") {
                project.system_prompt = v.as_str().map(|s| s.to_string());
            }
            if let Some(v) = params.get("description") {
                project.description = v.as_str().map(|s| s.to_string());
            }
            if let Some(v) = params.get("shell_enabled").and_then(|v| v.as_bool()) {
                project.shell_enabled = v;
            }
            if let Some(v) = params.get("coding_tools_enabled").and_then(|v| v.as_bool()) {
                project.coding_tools_enabled = v;
            }

            store.update(&project).await?;
            Ok(serde_json::json!({ "success": true }))
        }

        "projects.delete" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;

            if name == "default" {
                return Err(anyhow::anyhow!("Cannot delete the default project"));
            }

            {
                let store = state.project_store.write().await;
                store.delete(name).await?;
            }

            // Also delete sessions
            let _ = state.sessions.delete_project(name).await;

            Ok(serde_json::json!({ "success": true }))
        }

        // ── Approval management ──────────────────────────────────

        "approvals.list" => {
            let pending = state.approvals.list_pending().await;
            let items: Vec<serde_json::Value> = pending
                .into_iter()
                .map(|a| {
                    serde_json::json!({
                        "id": a.id,
                        "user_id": a.user_id,
                        "user_name": a.user_name,
                        "channel": a.channel,
                        "chat_id": a.chat_id,
                        "first_message": a.first_message,
                        "timestamp": a.timestamp.to_rfc3339(),
                    })
                })
                .collect();
            Ok(serde_json::json!({ "approvals": items }))
        }

        "approvals.approve" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;

            if let Some(approval) = state.approvals.approve(id).await {
                let _ = state.event_bus.send(BroadcastEvent::ApprovalResolved {
                    id: id.to_string(),
                    user_id: approval.user_id,
                    action: "approved".to_string(),
                });
                Ok(serde_json::json!({ "success": true }))
            } else {
                Err(anyhow::anyhow!("Approval not found: {}", id))
            }
        }

        "approvals.reject" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;

            if let Some(approval) = state.approvals.reject(id).await {
                let _ = state.event_bus.send(BroadcastEvent::ApprovalResolved {
                    id: id.to_string(),
                    user_id: approval.user_id,
                    action: "rejected".to_string(),
                });
                Ok(serde_json::json!({ "success": true }))
            } else {
                Err(anyhow::anyhow!("Approval not found: {}", id))
            }
        }

        _ => Err(anyhow::anyhow!("Unknown method: {}", method)),
    }
}
