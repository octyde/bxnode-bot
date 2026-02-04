//! Integration tests for WebSocket functionality

use bxnode_bot::gateway::protocol::{
    AuthScope, ConnectParams, ErrorShape, EventFrame, HealthState, HealthStatus, RequestFrame,
    ResponseFrame,
};
use serde_json::json;

// Note: Full WebSocket integration tests require a running server.
// These tests focus on protocol serialization/deserialization.

#[test]
fn test_request_response_flow() {
    // Simulate a request
    let request = RequestFrame {
        id: "req-001".to_string(),
        method: "health.get".to_string(),
        params: json!({}),
    };

    // Serialize to JSON (as it would be sent over WebSocket)
    let request_json = serde_json::to_string(&request).unwrap();

    // Parse the request (as the server would)
    let parsed_request: RequestFrame = serde_json::from_str(&request_json).unwrap();
    assert_eq!(parsed_request.id, "req-001");
    assert_eq!(parsed_request.method, "health.get");

    // Create response (as server would)
    let response = ResponseFrame {
        id: parsed_request.id,
        result: Some(json!({
            "status": "ok",
            "channels": [],
            "providers": []
        })),
        error: None,
    };

    // Serialize response
    let response_json = serde_json::to_string(&response).unwrap();

    // Parse response (as client would)
    let parsed_response: ResponseFrame = serde_json::from_str(&response_json).unwrap();
    assert_eq!(parsed_response.id, "req-001");
    assert!(parsed_response.result.is_some());
    assert!(parsed_response.error.is_none());
}

#[test]
fn test_error_response_flow() {
    let request = RequestFrame {
        id: "req-002".to_string(),
        method: "unknown.method".to_string(),
        params: json!({}),
    };

    // Server creates error response
    let response = ResponseFrame {
        id: request.id.clone(),
        result: None,
        error: Some(ErrorShape::method_not_found(&request.method)),
    };

    let response_json = serde_json::to_string(&response).unwrap();
    let parsed: ResponseFrame = serde_json::from_str(&response_json).unwrap();

    assert_eq!(parsed.id, "req-002");
    assert!(parsed.result.is_none());

    let error = parsed.error.unwrap();
    assert_eq!(error.code, -32601);
    assert!(error.message.contains("unknown.method"));
}

#[test]
fn test_event_broadcast() {
    let event = EventFrame {
        event: "session.message".to_string(),
        payload: json!({
            "session_id": "sess-123",
            "message": {
                "role": "assistant",
                "content": "Hello!"
            }
        }),
    };

    let event_json = serde_json::to_string(&event).unwrap();
    let parsed: EventFrame = serde_json::from_str(&event_json).unwrap();

    assert_eq!(parsed.event, "session.message");
    assert_eq!(parsed.payload["session_id"], "sess-123");
}

#[test]
fn test_connect_params_with_auth() {
    let params = ConnectParams {
        client_id: Some("client-abc".to_string()),
        version: Some("1.0.0".to_string()),
        token: Some("Bearer secret-token".to_string()),
        capabilities: vec!["streaming".to_string(), "tools".to_string()],
    };

    let json = serde_json::to_string(&params).unwrap();
    let parsed: ConnectParams = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.client_id, Some("client-abc".to_string()));
    assert_eq!(parsed.token, Some("Bearer secret-token".to_string()));
    assert_eq!(parsed.capabilities.len(), 2);
}

#[test]
fn test_auth_scopes() {
    let scopes = vec![
        AuthScope::Admin,
        AuthScope::Read,
        AuthScope::Write,
        AuthScope::Approvals,
        AuthScope::Pairing,
        AuthScope::Node,
    ];

    for scope in scopes {
        let json = serde_json::to_value(&scope).unwrap();
        let restored: AuthScope = serde_json::from_value(json).unwrap();
        assert_eq!(restored, scope);
    }
}

#[test]
fn test_health_state_in_event() {
    let health = HealthState {
        status: HealthStatus::Degraded,
        channels: vec![],
        providers: vec![],
    };

    let event = EventFrame {
        event: "health.changed".to_string(),
        payload: serde_json::to_value(&health).unwrap(),
    };

    let json = serde_json::to_string(&event).unwrap();
    let parsed: EventFrame = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.event, "health.changed");

    let health_payload: HealthState = serde_json::from_value(parsed.payload).unwrap();
    assert_eq!(health_payload.status, HealthStatus::Degraded);
}

#[test]
fn test_rpc_method_routing() {
    // Test various RPC methods that would be handled
    let methods = vec![
        ("health.get", json!({})),
        ("session.create", json!({"channel": "web", "agent_id": "default"})),
        ("session.list", json!({})),
        ("session.get", json!({"id": "sess-123"})),
        ("chat.send", json!({"session_id": "sess-123", "content": "Hello"})),
        ("config.get", json!({})),
        ("models.list", json!({})),
    ];

    for (method, params) in methods {
        let request = RequestFrame {
            id: format!("req-{}", method.replace('.', "-")),
            method: method.to_string(),
            params,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: RequestFrame = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.method, method);
    }
}

#[test]
fn test_batch_request_simulation() {
    // Simulate multiple requests in sequence
    let requests: Vec<RequestFrame> = (0..5)
        .map(|i| RequestFrame {
            id: format!("batch-{}", i),
            method: "ping".to_string(),
            params: json!({"seq": i}),
        })
        .collect();

    let responses: Vec<ResponseFrame> = requests
        .iter()
        .map(|req| ResponseFrame {
            id: req.id.clone(),
            result: Some(json!({"pong": req.params["seq"]})),
            error: None,
        })
        .collect();

    // Verify all responses match requests
    for (req, res) in requests.iter().zip(responses.iter()) {
        assert_eq!(req.id, res.id);
        assert!(res.result.is_some());
    }
}

#[test]
fn test_error_with_data() {
    let error = ErrorShape {
        code: -32000,
        message: "Rate limit exceeded".to_string(),
        data: Some(json!({
            "retry_after": 60,
            "limit": 100,
            "current": 105
        })),
    };

    let json = serde_json::to_string(&error).unwrap();
    let parsed: ErrorShape = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.code, -32000);
    assert_eq!(parsed.data.as_ref().unwrap()["retry_after"], 60);
}

#[test]
fn test_streaming_event_sequence() {
    // Simulate a streaming response as a sequence of events
    let events = vec![
        EventFrame {
            event: "stream.start".to_string(),
            payload: json!({"session_id": "sess-123"}),
        },
        EventFrame {
            event: "stream.chunk".to_string(),
            payload: json!({"content": "Hello"}),
        },
        EventFrame {
            event: "stream.chunk".to_string(),
            payload: json!({"content": " World"}),
        },
        EventFrame {
            event: "stream.end".to_string(),
            payload: json!({"finish_reason": "stop"}),
        },
    ];

    for event in &events {
        let json = serde_json::to_string(event).unwrap();
        let parsed: EventFrame = serde_json::from_str(&json).unwrap();
        assert!(parsed.event.starts_with("stream."));
    }
}

#[test]
fn test_complex_params_structure() {
    let request = RequestFrame {
        id: "complex-001".to_string(),
        method: "agent.configure".to_string(),
        params: json!({
            "agent_id": "custom-agent",
            "config": {
                "model": "claude-3-opus",
                "temperature": 0.7,
                "tools": ["search", "calculator", "code_interpreter"],
                "system_prompt": "You are a helpful assistant.",
                "context_window": 100000,
                "max_tokens": 4096
            },
            "metadata": {
                "owner": "user-123",
                "tags": ["production", "chat"]
            }
        }),
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: RequestFrame = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.params["config"]["model"], "claude-3-opus");
    assert_eq!(parsed.params["config"]["tools"].as_array().unwrap().len(), 3);
}

#[test]
fn test_welcome_message_structure() {
    // The welcome message sent when a client connects
    let welcome = EventFrame {
        event: "welcome".to_string(),
        payload: json!({
            "server_version": "0.1.0",
            "capabilities": ["streaming", "tools", "multi-model"],
            "session_timeout": 3600
        }),
    };

    let json = serde_json::to_string(&welcome).unwrap();
    assert!(json.contains("welcome"));
    assert!(json.contains("server_version"));
}

#[test]
fn test_connection_close_event() {
    let close_event = EventFrame {
        event: "connection.close".to_string(),
        payload: json!({
            "reason": "idle_timeout",
            "message": "Connection closed due to inactivity"
        }),
    };

    let json = serde_json::to_string(&close_event).unwrap();
    let parsed: EventFrame = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.event, "connection.close");
    assert_eq!(parsed.payload["reason"], "idle_timeout");
}
