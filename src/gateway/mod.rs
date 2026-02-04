//! Gateway module - HTTP and WebSocket server

pub mod http;
pub mod protocol;
pub mod ws;

pub mod methods;

#[cfg(test)]
mod protocol_tests;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::cli::ServeArgs;

/// Embedded UI assets
#[derive(rust_embed::RustEmbed)]
#[folder = "ui/dist/"]
struct UiAssets;

/// Start the gateway server
pub async fn serve(args: ServeArgs) -> Result<()> {
    let _config = crate::Config::load_or_default(args.config.as_ref());

    // Build the router
    let app = Router::new()
        // Health check
        .route("/health", get(http::health))
        // OpenAI-compatible API
        .route("/v1/chat/completions", post(http::chat_completions))
        .route("/v1/models", get(http::list_models))
        // WebSocket endpoint
        .route("/ws", get(ws::handler))
        // Static UI files
        .fallback(http::serve_ui)
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    // Bind and serve
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Gateway listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
