//! Integration tests for HTTP endpoints

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use bxnode_bot::channels::ChannelRegistry;
use bxnode_bot::gateway::AppState;
use bxnode_bot::providers::ProviderRegistry;

/// Create the test app router with empty registry
fn create_test_app() -> axum::Router {
    use axum::routing::{get, post};
    use bxnode_bot::gateway::http;

    let providers = ProviderRegistry::new();
    let channels = ChannelRegistry::new();
    let state = AppState {
        providers: Arc::new(providers),
        channels: Arc::new(channels),
    };

    axum::Router::new()
        .route("/health", get(http::health))
        .route("/v1/models", get(http::list_models))
        .route("/v1/chat/completions", post(http::chat_completions))
        .with_state(state)
}

/// Create the test app with Ollama provider (for more complete testing)
fn create_test_app_with_ollama() -> axum::Router {
    use axum::routing::{get, post};
    use bxnode_bot::gateway::http;
    use bxnode_bot::providers::ollama::{OllamaConfig, OllamaProvider};

    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(OllamaProvider::new(OllamaConfig::default())));
    let channels = ChannelRegistry::new();

    let state = AppState {
        providers: Arc::new(providers),
        channels: Arc::new(channels),
    };

    axum::Router::new()
        .route("/health", get(http::health))
        .route("/v1/models", get(http::list_models))
        .route("/v1/chat/completions", post(http::chat_completions))
        .with_state(state)
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
async fn test_list_models_empty_registry() {
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

    // Empty registry should return empty list
    let models = json["data"].as_array().unwrap();
    assert!(models.is_empty());
}

#[tokio::test]
async fn test_list_models_with_provider() {
    let app = create_test_app_with_ollama();

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
    let models = json["data"].as_array().unwrap();
    assert!(!models.is_empty());

    // Check model structure
    let first_model = &models[0];
    assert!(first_model["id"].is_string());
    assert_eq!(first_model["object"], "model");
    assert_eq!(first_model["owned_by"], "ollama");
}

#[tokio::test]
async fn test_chat_completions_no_provider() {
    let app = create_test_app();

    let request_body = json!({
        "model": "unknown/model",
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

    // Should return error when no provider is found
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["error"]["message"].as_str().unwrap().contains("not found"));
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
async fn test_models_response_structure_with_provider() {
    let app = create_test_app_with_ollama();

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
