//! Protocol definitions for gateway communication

use serde::{Deserialize, Serialize};

/// Connection parameters sent during WebSocket handshake
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectParams {
    /// Client identifier
    pub client_id: Option<String>,

    /// Client version
    pub version: Option<String>,

    /// Authorization token
    pub token: Option<String>,

    /// Requested capabilities
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Authorization scopes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthScope {
    Admin,
    Read,
    Write,
    Approvals,
    Pairing,
    Node,
}

/// Request frame from client
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestFrame {
    /// Request ID for correlation
    pub id: String,

    /// RPC method name
    pub method: String,

    /// Method parameters
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Response frame to client
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseFrame {
    /// Request ID (correlates to RequestFrame.id)
    pub id: String,

    /// Result value (if success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    /// Error (if failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorShape>,
}

/// Event frame (server-initiated)
#[derive(Debug, Serialize, Deserialize)]
pub struct EventFrame {
    /// Event name
    pub event: String,

    /// Event payload
    pub payload: serde_json::Value,
}

/// Error shape for RPC errors
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorShape {
    /// Error code
    pub code: i32,

    /// Error message
    pub message: String,

    /// Additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ErrorShape {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("Method not found: {}", method))
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }
}

/// Health state snapshot
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthState {
    /// Overall status
    pub status: HealthStatus,

    /// Channel statuses
    #[serde(default)]
    pub channels: Vec<ChannelHealth>,

    /// Provider statuses
    #[serde(default)]
    pub providers: Vec<ProviderHealth>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    #[default]
    Ok,
    Degraded,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelHealth {
    pub id: String,
    pub status: HealthStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub id: String,
    pub status: HealthStatus,
    pub message: Option<String>,
}
