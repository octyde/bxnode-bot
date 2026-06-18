//! Unit tests for the providers module

use super::*;
use serde_json::json;

#[test]
fn test_message_creation() {
    let msg = Message::text(Role::User, "Hello, assistant!");

    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content, "Hello, assistant!");
}

#[test]
fn test_role_serialization() {
    assert_eq!(serde_json::to_value(Role::System).unwrap(), json!("system"));
    assert_eq!(serde_json::to_value(Role::User).unwrap(), json!("user"));
    assert_eq!(serde_json::to_value(Role::Assistant).unwrap(), json!("assistant"));
}

#[test]
fn test_role_deserialization() {
    assert_eq!(
        serde_json::from_value::<Role>(json!("system")).unwrap(),
        Role::System
    );
    assert_eq!(
        serde_json::from_value::<Role>(json!("user")).unwrap(),
        Role::User
    );
    assert_eq!(
        serde_json::from_value::<Role>(json!("assistant")).unwrap(),
        Role::Assistant
    );
}

#[test]
fn test_message_serialization() {
    let msg = Message::text(Role::Assistant, "How can I help you?");

    let json = serde_json::to_value(&msg).unwrap();

    assert_eq!(json["role"], "assistant");
    assert_eq!(json["content"], "How can I help you?");
}

#[test]
fn test_completion_request_basic() {
    let request = CompletionRequest {
        model: "claude-3-opus".to_string(),
        messages: vec![Message::text(Role::User, "Hello")],
        temperature: None,
        max_tokens: None,
        stop: vec![],
        stream: false,
        tools: vec![],
    };

    assert_eq!(request.model, "claude-3-opus");
    assert_eq!(request.messages.len(), 1);
    assert!(!request.stream);
}

#[test]
fn test_completion_request_serialization() {
    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            Message::text(Role::System, "You are a helpful assistant."),
            Message::text(Role::User, "What is 2+2?"),
        ],
        temperature: Some(0.7),
        max_tokens: Some(1000),
        stop: vec!["STOP".to_string()],
        stream: true,
        tools: vec![],
    };

    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["model"], "gpt-4");
    assert_eq!(json["messages"].as_array().unwrap().len(), 2);
    // Use approximate comparison for f32 to avoid floating point precision issues
    let temp = json["temperature"].as_f64().unwrap();
    assert!((temp - 0.7).abs() < 0.001, "temperature mismatch: {}", temp);
    assert_eq!(json["max_tokens"], 1000);
    assert_eq!(json["stop"][0], "STOP");
    assert_eq!(json["stream"], true);
}

#[test]
fn test_completion_request_deserialization() {
    let json = json!({
        "model": "llama3",
        "messages": [
            { "role": "user", "content": "Hi!" }
        ],
        "stream": false
    });

    let request: CompletionRequest = serde_json::from_value(json).unwrap();

    assert_eq!(request.model, "llama3");
    assert_eq!(request.messages.len(), 1);
    assert!(request.temperature.is_none());
    assert!(request.max_tokens.is_none());
    assert!(request.stop.is_empty());
    assert!(!request.stream);
}

#[test]
fn test_completion_response() {
    let response = CompletionResponse {
        content: "The answer is 4.".to_string(),
        model: "gpt-4".to_string(),
        finish_reason: FinishReason::Stop,
        usage: Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        },
        tool_calls: vec![],
    };

    assert_eq!(response.content, "The answer is 4.");
    assert_eq!(response.model, "gpt-4");
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert_eq!(response.usage.total_tokens, 15);
}

#[test]
fn test_finish_reason_serialization() {
    assert_eq!(serde_json::to_value(FinishReason::Stop).unwrap(), json!("stop"));
    assert_eq!(serde_json::to_value(FinishReason::Length).unwrap(), json!("length"));
    assert_eq!(serde_json::to_value(FinishReason::ToolUse).unwrap(), json!("tool_use"));
    assert_eq!(
        serde_json::to_value(FinishReason::ContentFilter).unwrap(),
        json!("content_filter")
    );
}

#[test]
fn test_finish_reason_deserialization() {
    assert_eq!(
        serde_json::from_value::<FinishReason>(json!("stop")).unwrap(),
        FinishReason::Stop
    );
    assert_eq!(
        serde_json::from_value::<FinishReason>(json!("length")).unwrap(),
        FinishReason::Length
    );
    assert_eq!(
        serde_json::from_value::<FinishReason>(json!("tool_use")).unwrap(),
        FinishReason::ToolUse
    );
}

#[test]
fn test_usage_default() {
    let usage = Usage::default();

    assert_eq!(usage.prompt_tokens, 0);
    assert_eq!(usage.completion_tokens, 0);
    assert_eq!(usage.total_tokens, 0);
}

#[test]
fn test_usage_serialization() {
    let usage = Usage {
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
    };

    let json = serde_json::to_value(&usage).unwrap();

    assert_eq!(json["prompt_tokens"], 100);
    assert_eq!(json["completion_tokens"], 50);
    assert_eq!(json["total_tokens"], 150);
}

#[test]
fn test_model_info() {
    let model = ModelInfo {
        id: "claude-3-opus-20240229".to_string(),
        name: "Claude 3 Opus".to_string(),
        context_length: 200000,
        capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string()],
    };

    assert_eq!(model.id, "claude-3-opus-20240229");
    assert_eq!(model.name, "Claude 3 Opus");
    assert_eq!(model.context_length, 200000);
    assert_eq!(model.capabilities.len(), 3);
}

#[test]
fn test_model_info_serialization() {
    let model = ModelInfo {
        id: "gpt-4-turbo".to_string(),
        name: "GPT-4 Turbo".to_string(),
        context_length: 128000,
        capabilities: vec!["chat".to_string(), "function_calling".to_string()],
    };

    let json = serde_json::to_value(&model).unwrap();

    assert_eq!(json["id"], "gpt-4-turbo");
    assert_eq!(json["name"], "GPT-4 Turbo");
    assert_eq!(json["context_length"], 128000);
    assert_eq!(json["capabilities"].as_array().unwrap().len(), 2);
}

#[test]
fn test_completion_response_serialization_roundtrip() {
    let response = CompletionResponse {
        content: "Test response content.".to_string(),
        model: "test-model".to_string(),
        finish_reason: FinishReason::Stop,
        usage: Usage {
            prompt_tokens: 25,
            completion_tokens: 10,
            total_tokens: 35,
        },
        tool_calls: vec![],
    };

    let json = serde_json::to_value(&response).unwrap();
    let restored: CompletionResponse = serde_json::from_value(json).unwrap();

    assert_eq!(restored.content, response.content);
    assert_eq!(restored.model, response.model);
    assert_eq!(restored.finish_reason, response.finish_reason);
    assert_eq!(restored.usage.total_tokens, response.usage.total_tokens);
}

#[test]
fn test_message_deserialization() {
    let json = json!({
        "role": "user",
        "content": "Tell me a joke"
    });

    let msg: Message = serde_json::from_value(json).unwrap();

    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content, "Tell me a joke");
}

#[test]
fn test_completion_request_with_all_fields() {
    let request = CompletionRequest {
        model: "claude-3-sonnet".to_string(),
        messages: vec![
            Message::text(Role::System, "Be concise."),
            Message::text(Role::User, "Summarize AI."),
        ],
        temperature: Some(0.5),
        max_tokens: Some(500),
        stop: vec!["END".to_string(), "STOP".to_string()],
        stream: true,
        tools: vec![],
    };

    let json = serde_json::to_value(&request).unwrap();
    let restored: CompletionRequest = serde_json::from_value(json).unwrap();

    assert_eq!(restored.model, "claude-3-sonnet");
    assert_eq!(restored.messages.len(), 2);
    assert_eq!(restored.temperature, Some(0.5));
    assert_eq!(restored.max_tokens, Some(500));
    assert_eq!(restored.stop.len(), 2);
    assert!(restored.stream);
}
