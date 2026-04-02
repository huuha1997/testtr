use std::net::SocketAddr;

use axum::{
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct McpRequest {
    provider: String,
    payload: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct McpResponse {
    accepted: bool,
    provider: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let http_addr: SocketAddr = std::env::var("HTTP_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8090".to_string())
        .parse()?;

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/mcp/design/generate", post(handle_mcp))
        .route("/mcp/spec/extract", post(handle_mcp))
        .route("/mcp/codegen/run", post(handle_mcp))
        .route("/mcp/repo/create-pr", post(handle_mcp))
        .route("/mcp/deploy/vercel", post(handle_mcp))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    info!(%http_addr, "mcp-gateway listening");
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn handle_mcp(Json(payload): Json<McpRequest>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(McpResponse {
            accepted: true,
            provider: payload.provider,
        }),
    )
}
