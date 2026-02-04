//! Unit tests for the protocol module

use super::protocol::*;
use serde_json::json;

#[test]
fn test_connect_params_deserialization() {
    let json = json!({
        "client_id": "client-123",
        "version": "1.0.0",
        "token": "auth-token",
        "capabilities": ["streaming", "tools"]
    });

    let params: ConnectParams = serde_json::from_value(json).unwrap();

    assert_eq!(params.client_id, Some("client-123".to_string()));
    assert_eq!(params.version, Some("1.0.0".to_string()));
    assert_eq!(params.token, Some("auth-token".to_string()));
    assert_eq!(params.capabilities, vec!["streaming", "tools"]);
}

#[test]
fn test_connect_params_with_defaults() {
    let json = json!({});

    let params: ConnectParams = serde_json::from_value(json).unwrap();

    assert!(params.client_id.is_none());
    assert!(params.version.is_none());
    assert!(params.token.is_none());
    assert!(params.capabilities.is_empty());
}

#[test]
fn test_auth_scope_serialization() {
    assert_eq!(serde_json::to_value(AuthScope::Admin).unwrap(), json!("admin"));
    assert_eq!(serde_json::to_value(AuthScope::Read).unwrap(), json!("read"));
    assert_eq!(serde_json::to_value(AuthScope::Write).unwrap(), json!("write"));
    assert_eq!(serde_json::to_value(AuthScope::Approvals).unwrap(), json!("approvals"));
    assert_eq!(serde_json::to_value(AuthScope::Pairing).unwrap(), json!("pairing"));
    assert_eq!(serde_json::to_value(AuthScope::Node).unwrap(), json!("node"));
}

#[test]
fn test_auth_scope_deserialization() {
    assert_eq!(
        serde_json::from_value::<AuthScope>(json!("admin")).unwrap(),
        AuthScope::Admin
    );
    assert_eq!(
        serde_json::from_value::<AuthScope>(json!("read")).unwrap(),
        AuthScope::Read
    );
}

#[test]
fn test_request_frame_serialization() {
    let request = RequestFrame {
        id: "req-001".to_string(),
        method: "chat.send".to_string(),
        params: json!({ "message": "Hello" }),
    };

    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["id"], "req-001");
    assert_eq!(json["method"], "chat.send");
    assert_eq!(json["params"]["message"], "Hello");
}

#[test]
fn test_request_frame_deserialization() {
    let json = json!({
        "id": "req-002",
        "method": "session.create",
        "params": {
            "agent_id": "default",
            "channel": "web"
        }
    });

    let request: RequestFrame = serde_json::from_value(json).unwrap();

    assert_eq!(request.id, "req-002");
    assert_eq!(request.method, "session.create");
    assert_eq!(request.params["agent_id"], "default");
}

#[test]
fn test_request_frame_with_empty_params() {
    let json = json!({
        "id": "req-003",
        "method": "status.get"
    });

    let request: RequestFrame = serde_json::from_value(json).unwrap();

    assert_eq!(request.id, "req-003");
    assert_eq!(request.method, "status.get");
    assert!(request.params.is_null());
}

#[test]
fn test_response_frame_success() {
    let response = ResponseFrame {
        id: "req-001".to_string(),
        result: Some(json!({ "status": "ok" })),
        error: None,
    };

    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["id"], "req-001");
    assert!(json["result"].is_object());
    assert!(json.get("error").is_none()); // Should be skipped due to skip_serializing_if
}

#[test]
fn test_response_frame_error() {
    let response = ResponseFrame {
        id: "req-002".to_string(),
        result: None,
        error: Some(ErrorShape {
            code: -32600,
            message: "Invalid request".to_string(),
            data: None,
        }),
    };

    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["id"], "req-002");
    assert!(json.get("result").is_none());
    assert_eq!(json["error"]["code"], -32600);
    assert_eq!(json["error"]["message"], "Invalid request");
}

#[test]
fn test_event_frame_serialization() {
    let event = EventFrame {
        event: "session.updated".to_string(),
        payload: json!({
            "session_id": "sess-123",
            "status": "active"
        }),
    };

    let json = serde_json::to_value(&event).unwrap();

    assert_eq!(json["event"], "session.updated");
    assert_eq!(json["payload"]["session_id"], "sess-123");
}

#[test]
fn test_error_shape_new() {
    let error = ErrorShape::new(100, "Custom error");

    assert_eq!(error.code, 100);
    assert_eq!(error.message, "Custom error");
    assert!(error.data.is_none());
}

#[test]
fn test_error_shape_method_not_found() {
    let error = ErrorShape::method_not_found("unknown.method");

    assert_eq!(error.code, -32601);
    assert_eq!(error.message, "Method not found: unknown.method");
}

#[test]
fn test_error_shape_invalid_params() {
    let error = ErrorShape::invalid_params("Missing required field: name");

    assert_eq!(error.code, -32602);
    assert_eq!(error.message, "Missing required field: name");
}

#[test]
fn test_error_shape_internal_error() {
    let error = ErrorShape::internal_error("Database connection failed");

    assert_eq!(error.code, -32603);
    assert_eq!(error.message, "Database connection failed");
}

#[test]
fn test_health_state_default() {
    let health = HealthState::default();

    assert_eq!(health.status, HealthStatus::Ok);
    assert!(health.channels.is_empty());
    assert!(health.providers.is_empty());
}

#[test]
fn test_health_status_serialization() {
    assert_eq!(serde_json::to_value(HealthStatus::Ok).unwrap(), json!("ok"));
    assert_eq!(serde_json::to_value(HealthStatus::Degraded).unwrap(), json!("degraded"));
    assert_eq!(serde_json::to_value(HealthStatus::Error).unwrap(), json!("error"));
}

#[test]
fn test_health_state_with_channels_and_providers() {
    let health = HealthState {
        status: HealthStatus::Degraded,
        channels: vec![
            ChannelHealth {
                id: "telegram".to_string(),
                status: HealthStatus::Ok,
                message: None,
            },
            ChannelHealth {
                id: "discord".to_string(),
                status: HealthStatus::Error,
                message: Some("Connection timeout".to_string()),
            },
        ],
        providers: vec![ProviderHealth {
            id: "anthropic".to_string(),
            status: HealthStatus::Ok,
            message: None,
        }],
    };

    let json = serde_json::to_value(&health).unwrap();

    assert_eq!(json["status"], "degraded");
    assert_eq!(json["channels"].as_array().unwrap().len(), 2);
    assert_eq!(json["providers"].as_array().unwrap().len(), 1);
    assert_eq!(json["channels"][1]["message"], "Connection timeout");
}

#[test]
fn test_channel_health_serialization() {
    let channel = ChannelHealth {
        id: "slack".to_string(),
        status: HealthStatus::Ok,
        message: Some("Connected".to_string()),
    };

    let json = serde_json::to_value(&channel).unwrap();

    assert_eq!(json["id"], "slack");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["message"], "Connected");
}

#[test]
fn test_provider_health_deserialization() {
    let json = json!({
        "id": "openai",
        "status": "error",
        "message": "Rate limit exceeded"
    });

    let provider: ProviderHealth = serde_json::from_value(json).unwrap();

    assert_eq!(provider.id, "openai");
    assert_eq!(provider.status, HealthStatus::Error);
    assert_eq!(provider.message, Some("Rate limit exceeded".to_string()));
}
