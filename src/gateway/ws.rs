//! WebSocket handler for RPC communication

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use super::AppState;

/// WebSocket upgrade handler
pub async fn handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle WebSocket connection
async fn handle_socket(mut socket: WebSocket, state: AppState) {
    tracing::info!("New WebSocket connection");

    // Send welcome message
    let welcome = RpcMessage::Event {
        event: "connected".to_string(),
        payload: serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "server": "bxnode-bot"
        }),
    };

    if let Err(e) = socket
        .send(Message::Text(serde_json::to_string(&welcome).unwrap().into()))
        .await
    {
        tracing::error!("Failed to send welcome: {}", e);
        return;
    }

    // Handle incoming messages
    while let Some(msg) = socket.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Err(e) = handle_message(&mut socket, &text, &state).await {
                    tracing::error!("Error handling message: {}", e);
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    if let Err(e) = handle_message(&mut socket, &text, &state).await {
                        tracing::error!("Error handling binary message: {}", e);
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                let _ = socket.send(Message::Pong(data)).await;
            }
            Ok(Message::Close(_)) => {
                tracing::info!("WebSocket closed by client");
                break;
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
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
    socket: &mut WebSocket,
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

            socket
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
    _params: serde_json::Value,
    state: &AppState,
) -> anyhow::Result<serde_json::Value> {
    match method {
        "health" => Ok(serde_json::json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION")
        })),

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

        "sessions.list" => Ok(serde_json::json!({
            "sessions": []
        })),

        _ => Err(anyhow::anyhow!("Unknown method: {}", method)),
    }
}
