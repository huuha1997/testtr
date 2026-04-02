use std::{net::SocketAddr, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};

#[derive(Clone)]
struct AppState {
    http: reqwest::Client,
    internal_api_key: Option<String>,
    provider_max_retries: usize,
    banana: ProviderProxyConfig,
    stitch: ProviderProxyConfig,
    claude: ProviderProxyConfig,
    github_api_base: String,
    github_api_token: Option<String>,
    vercel_api_base: String,
    vercel_api_token: Option<String>,
}

#[derive(Clone)]
struct ProviderProxyConfig {
    name: &'static str,
    api_url: Option<String>,
    api_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ProxyBody {
    Envelope { payload: serde_json::Value },
    Raw(serde_json::Value),
}

#[derive(Debug, Serialize)]
struct ProviderProxyResponse {
    accepted: bool,
    provider: String,
    attempt_count: usize,
    raw: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GithubCreatePrRequest {
    owner: String,
    repo: String,
    head: String,
    base: String,
    title: String,
    body: Option<String>,
    draft: Option<bool>,
}

#[derive(Debug, Serialize)]
struct GithubCreatePrResponse {
    accepted: bool,
    number: i64,
    state: Option<String>,
    html_url: Option<String>,
    attempt_count: usize,
    raw: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GithubPrApiResponse {
    number: i64,
    state: Option<String>,
    html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VercelDeployRequest {
    team_id: Option<String>,
    slug: Option<String>,
    deployment: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct VercelDeployResponse {
    accepted: bool,
    deployment_id: String,
    ready_state: Option<String>,
    deployment_url: Option<String>,
    inspector_url: Option<String>,
    attempt_count: usize,
    raw: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct VercelDeploymentApiResponse {
    id: String,
    url: Option<String>,
    #[serde(rename = "readyState")]
    ready_state: Option<String>,
    #[serde(rename = "inspectorUrl")]
    inspector_url: Option<String>,
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
    let provider_timeout_seconds: u64 = std::env::var("MCP_PROVIDER_HTTP_TIMEOUT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);
    let provider_max_retries: usize = std::env::var("MCP_PROVIDER_MAX_RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let state = AppState {
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(provider_timeout_seconds))
            .build()?,
        internal_api_key: std::env::var("MCP_INTERNAL_API_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty()),
        provider_max_retries,
        banana: ProviderProxyConfig {
            name: "banana",
            api_url: std::env::var("BANANA_API_URL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            api_token: std::env::var("BANANA_API_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
        },
        stitch: ProviderProxyConfig {
            name: "stitch",
            api_url: std::env::var("STITCH_API_URL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            api_token: std::env::var("STITCH_API_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
        },
        claude: ProviderProxyConfig {
            name: "claude",
            api_url: std::env::var("CLAUDE_API_URL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            api_token: std::env::var("CLAUDE_API_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
        },
        github_api_base: std::env::var("GITHUB_API_BASE")
            .unwrap_or_else(|_| "https://api.github.com".to_string()),
        github_api_token: std::env::var("GITHUB_API_TOKEN")
            .ok()
            .filter(|v| !v.trim().is_empty()),
        vercel_api_base: std::env::var("VERCEL_API_BASE")
            .unwrap_or_else(|_| "https://api.vercel.com".to_string()),
        vercel_api_token: std::env::var("VERCEL_API_TOKEN")
            .ok()
            .filter(|v| !v.trim().is_empty()),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/mcp/design/generate", post(handle_design_generate))
        .route("/mcp/spec/extract", post(handle_spec_extract))
        .route("/mcp/codegen/run", post(handle_codegen_run))
        .route("/mcp/repo/create-pr", post(handle_github_create_pr))
        .route("/mcp/deploy/vercel", post(handle_vercel_deploy))
        .with_state(state)
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

async fn handle_design_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProxyBody>,
) -> Result<Json<ProviderProxyResponse>, (StatusCode, Json<ErrorResponse>)> {
    authorize_internal(&state, &headers)?;
    proxy_provider_call(&state, &state.banana, extract_payload(body)).await
}

async fn handle_spec_extract(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProxyBody>,
) -> Result<Json<ProviderProxyResponse>, (StatusCode, Json<ErrorResponse>)> {
    authorize_internal(&state, &headers)?;
    proxy_provider_call(&state, &state.stitch, extract_payload(body)).await
}

async fn handle_codegen_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProxyBody>,
) -> Result<Json<ProviderProxyResponse>, (StatusCode, Json<ErrorResponse>)> {
    authorize_internal(&state, &headers)?;
    proxy_provider_call(&state, &state.claude, extract_payload(body)).await
}

async fn handle_github_create_pr(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GithubCreatePrRequest>,
) -> Result<Json<GithubCreatePrResponse>, (StatusCode, Json<ErrorResponse>)> {
    authorize_internal(&state, &headers)?;
    let token = state.github_api_token.clone().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "GITHUB_API_TOKEN is not configured".to_string(),
            }),
        )
    })?;

    let url = Url::parse(&format!(
        "{}/repos/{}/{}/pulls",
        state.github_api_base.trim_end_matches('/'),
        payload.owner,
        payload.repo
    ))
    .map_err(internal_err)?;

    let body = serde_json::json!({
        "title": payload.title,
        "head": payload.head,
        "base": payload.base,
        "body": payload.body,
        "draft": payload.draft.unwrap_or(false)
    });

    let (attempt_count, raw) = call_with_retry(
        &state,
        url,
        Some(token),
        Some(vec![("accept".to_string(), "application/vnd.github+json".to_string()), ("user-agent".to_string(), "agentic-mcp-gateway".to_string())]),
        body,
    )
    .await?;
    let parsed: GithubPrApiResponse = serde_json::from_value(raw.clone()).map_err(bad_gateway_err)?;

    Ok(Json(GithubCreatePrResponse {
        accepted: true,
        number: parsed.number,
        state: parsed.state,
        html_url: parsed.html_url,
        attempt_count,
        raw,
    }))
}

async fn handle_vercel_deploy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<VercelDeployRequest>,
) -> Result<Json<VercelDeployResponse>, (StatusCode, Json<ErrorResponse>)> {
    authorize_internal(&state, &headers)?;
    if !payload.deployment.is_object() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "deployment must be a JSON object".to_string(),
            }),
        ));
    }

    let token = state.vercel_api_token.clone().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "VERCEL_API_TOKEN is not configured".to_string(),
            }),
        )
    })?;

    let mut url = Url::parse(&format!(
        "{}/v13/deployments",
        state.vercel_api_base.trim_end_matches('/')
    ))
    .map_err(internal_err)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("forceNew", "1");
        if let Some(team_id) = payload.team_id.as_deref() {
            qp.append_pair("teamId", team_id);
        }
        if let Some(slug) = payload.slug.as_deref() {
            qp.append_pair("slug", slug);
        }
    }

    let (attempt_count, raw) = call_with_retry(&state, url, Some(token), None, payload.deployment).await?;
    let parsed: VercelDeploymentApiResponse =
        serde_json::from_value(raw.clone()).map_err(bad_gateway_err)?;
    let deployment_url = parsed.url.map(|u| {
        if u.starts_with("http://") || u.starts_with("https://") {
            u
        } else {
            format!("https://{}", u)
        }
    });
    Ok(Json(VercelDeployResponse {
        accepted: true,
        deployment_id: parsed.id,
        ready_state: parsed.ready_state,
        deployment_url,
        inspector_url: parsed.inspector_url,
        attempt_count,
        raw,
    }))
}

async fn proxy_provider_call(
    state: &AppState,
    provider: &ProviderProxyConfig,
    payload: serde_json::Value,
) -> Result<Json<ProviderProxyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let api_url = provider.api_url.clone().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{}_API_URL is not configured", provider.name.to_uppercase()),
            }),
        )
    })?;
    let token = provider.api_token.clone().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{}_API_TOKEN is not configured", provider.name.to_uppercase()),
            }),
        )
    })?;

    let url = Url::parse(&api_url).map_err(internal_err)?;
    let (attempt_count, raw) = call_with_retry(state, url, Some(token), None, payload).await?;
    Ok(Json(ProviderProxyResponse {
        accepted: true,
        provider: provider.name.to_string(),
        attempt_count,
        raw,
    }))
}

async fn call_with_retry(
    state: &AppState,
    url: Url,
    bearer_token: Option<String>,
    headers: Option<Vec<(String, String)>>,
    body: serde_json::Value,
) -> Result<(usize, serde_json::Value), (StatusCode, Json<ErrorResponse>)> {
    let mut last_error: Option<String> = None;
    for attempt in 1..=state.provider_max_retries {
        let mut req = state.http.post(url.clone()).json(&body);
        if let Some(token) = bearer_token.as_ref() {
            req = req.bearer_auth(token);
        }
        if let Some(headers) = headers.as_ref() {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_else(|_| "{}".to_string());
                if status.is_success() {
                    let raw = serde_json::from_str::<serde_json::Value>(&text).unwrap_or_else(|_| {
                        serde_json::json!({ "raw_text": text })
                    });
                    return Ok((attempt, raw));
                }
                let retriable =
                    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS;
                let err = format!("upstream {}: {}", status.as_u16(), text);
                if retriable && attempt < state.provider_max_retries {
                    warn!(attempt, max = state.provider_max_retries, error = %err, "retrying upstream call");
                    sleep(backoff_duration(attempt)).await;
                    last_error = Some(err);
                    continue;
                }
                return Err((
                    StatusCode::BAD_GATEWAY,
                    Json(ErrorResponse { error: err }),
                ));
            }
            Err(err) => {
                let err = format!("upstream request failed: {}", err);
                if attempt < state.provider_max_retries {
                    warn!(attempt, max = state.provider_max_retries, error = %err, "retrying upstream call");
                    sleep(backoff_duration(attempt)).await;
                    last_error = Some(err);
                    continue;
                }
                return Err((
                    StatusCode::BAD_GATEWAY,
                    Json(ErrorResponse { error: err }),
                ));
            }
        }
    }
    Err((
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse {
            error: format!(
                "upstream failed after retries: {}",
                last_error.unwrap_or_else(|| "unknown error".to_string())
            ),
        }),
    ))
}

fn extract_payload(body: ProxyBody) -> serde_json::Value {
    match body {
        ProxyBody::Envelope { payload } => payload,
        ProxyBody::Raw(v) => v,
    }
}

fn authorize_internal(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let Some(expected) = state.internal_api_key.as_ref() else {
        return Ok(());
    };
    let actual = headers
        .get("x-internal-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if actual == expected {
        return Ok(());
    }
    Err((
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: "invalid internal api key".to_string(),
        }),
    ))
}

fn internal_err(err: impl std::fmt::Display) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: err.to_string(),
        }),
    )
}

fn bad_gateway_err(err: impl std::fmt::Display) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse {
            error: err.to_string(),
        }),
    )
}

fn backoff_duration(attempt: usize) -> Duration {
    Duration::from_millis((attempt as u64) * 300)
}
