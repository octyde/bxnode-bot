//! Integration tests for HTTP endpoints

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

/// Create the test app router
fn create_test_app() -> axum::Router {
    use axum::routing::{get, post};
    use bxnode_bot::gateway::http;

    axum::Router::new()
        .route("/health", get(http::health))
        .route("/v1/models", get(http::list_models))
        .route("/v1/chat/completions", post(http::chat_completions))
}

#[tokio::test]
async fn test_health_endpoint() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn test_list_models_endpoint() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["object"], "list");
    assert!(json["data"].is_array());

    let models = json["data"].as_array().unwrap();
    assert!(!models.is_empty());

    // Check model structure
    let first_model = &models[0];
    assert!(first_model["id"].is_string());
    assert_eq!(first_model["object"], "model");
    assert!(first_model["owned_by"].is_string());
}

#[tokio::test]
async fn test_chat_completions_endpoint() {
    let app = create_test_app();

    let request_body = json!({
        "model": "anthropic/claude-3-opus",
        "messages": [
            {
                "role": "user",
                "content": "Hello!"
            }
        ],
        "stream": false
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Verify OpenAI-compatible response structure
    assert!(json["id"].is_string());
    assert_eq!(json["object"], "chat.completion");
    assert!(json["created"].is_number());
    assert_eq!(json["model"], "anthropic/claude-3-opus");
    assert!(json["choices"].is_array());
    assert!(json["usage"].is_object());

    // Check choices structure
    let choices = json["choices"].as_array().unwrap();
    assert!(!choices.is_empty());

    let choice = &choices[0];
    assert_eq!(choice["index"], 0);
    assert!(choice["message"].is_object());
    assert_eq!(choice["message"]["role"], "assistant");
    assert!(choice["message"]["content"].is_string());
    assert!(choice["finish_reason"].is_string());
}

#[tokio::test]
async fn test_chat_completions_with_system_message() {
    let app = create_test_app();

    let request_body = json!({
        "model": "openai/gpt-4",
        "messages": [
            {
                "role": "system",
                "content": "You are a helpful assistant."
            },
            {
                "role": "user",
                "content": "What is 2+2?"
            }
        ],
        "temperature": 0.7,
        "max_tokens": 100
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_chat_completions_invalid_json() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .body(Body::from("{ invalid json }"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 4xx for invalid JSON
    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn test_chat_completions_missing_required_fields() {
    let app = create_test_app();

    // Missing messages field
    let request_body = json!({
        "model": "test-model"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail validation
    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn test_health_returns_json_content_type() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""));

    assert!(content_type.is_some());
    assert!(content_type.unwrap().contains("application/json"));
}

#[tokio::test]
async fn test_models_response_structure() {
    let app = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Verify each model has required fields
    for model in json["data"].as_array().unwrap() {
        assert!(model["id"].is_string());
        assert!(model["object"].is_string());
        assert!(model["owned_by"].is_string());
    }
}

#[tokio::test]
async fn test_chat_completion_id_format() {
    let app = create_test_app();

    let request_body = json!({
        "model": "test",
        "messages": [{"role": "user", "content": "test"}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // ID should start with "chatcmpl-"
    let id = json["id"].as_str().unwrap();
    assert!(id.starts_with("chatcmpl-"));
}

#[tokio::test]
async fn test_usage_fields() {
    let app = create_test_app();

    let request_body = json!({
        "model": "test",
        "messages": [{"role": "user", "content": "test"}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let usage = &json["usage"];
    assert!(usage["prompt_tokens"].is_number());
    assert!(usage["completion_tokens"].is_number());
    assert!(usage["total_tokens"].is_number());
}
