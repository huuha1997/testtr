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
    api_key_header: Option<String>,
    extra_headers: Vec<(String, String)>,
    default_model: Option<String>,
    default_max_tokens: Option<u32>,
    mcp_tool_name: Option<String>,
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
            api_key_header: Some(
                std::env::var("BANANA_API_KEY_HEADER")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or_else(|| "x-goog-api-key".to_string()),
            ),
            extra_headers: parse_extra_headers("BANANA_API_EXTRA_HEADERS_JSON"),
            default_model: None,
            default_max_tokens: None,
            mcp_tool_name: None,
        },
        stitch: ProviderProxyConfig {
            name: "stitch",
            api_url: std::env::var("STITCH_API_URL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            api_token: std::env::var("STITCH_API_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            api_key_header: Some(
                std::env::var("STITCH_API_KEY_HEADER")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or_else(|| "x-api-key".to_string()),
            ),
            extra_headers: parse_extra_headers("STITCH_API_EXTRA_HEADERS_JSON"),
            default_model: None,
            default_max_tokens: None,
            mcp_tool_name: std::env::var("STITCH_MCP_TOOL_NAME")
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
            api_key_header: Some(
                std::env::var("CLAUDE_API_KEY_HEADER")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or_else(|| "x-api-key".to_string()),
            ),
            extra_headers: parse_extra_headers("CLAUDE_API_EXTRA_HEADERS_JSON"),
            default_model: std::env::var("CLAUDE_MODEL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            default_max_tokens: std::env::var("CLAUDE_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse::<u32>().ok()),
            mcp_tool_name: None,
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
        .route("/mcp/design/stitch-generate", post(handle_stitch_generate))
        .route("/mcp/design/get-code", post(handle_stitch_get_code))
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

async fn handle_stitch_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProxyBody>,
) -> Result<Json<ProviderProxyResponse>, (StatusCode, Json<ErrorResponse>)> {
    authorize_internal(&state, &headers)?;
    let payload = extract_payload(body);
    let normalized = normalize_stitch_tool_call(&state.stitch, "generate_screen_from_text", payload);
    proxy_provider_call(&state, &state.stitch, normalized).await
}

async fn handle_stitch_get_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProxyBody>,
) -> Result<Json<ProviderProxyResponse>, (StatusCode, Json<ErrorResponse>)> {
    authorize_internal(&state, &headers)?;
    let payload = extract_payload(body);
    let normalized = normalize_stitch_tool_call(&state.stitch, "get_screen_code", payload);
    proxy_provider_call(&state, &state.stitch, normalized).await
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
    let payload = normalize_provider_payload(provider, &url, payload);
    let mut extra_headers: Vec<(String, String)> = Vec::new();
    let bearer_token = if let Some(header_name) = provider.api_key_header.as_ref() {
        let value = if header_name.to_lowercase() == "authorization"
            && !token.to_lowercase().starts_with("bearer ")
        {
            format!("Bearer {}", token)
        } else {
            token
        };
        extra_headers.push((header_name.clone(), value));
        None
    } else {
        Some(token)
    };
    extra_headers.extend(provider.extra_headers.clone());
    let headers = if extra_headers.is_empty() { None } else { Some(extra_headers) };
    let (attempt_count, raw) =
        call_with_retry(state, url, bearer_token, headers, payload).await?;
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
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                if status.is_success() {
                    if content_type.starts_with("image/") {
                        use base64::Engine as _;
                        let bytes = resp.bytes().await.unwrap_or_default();
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        return Ok((attempt, serde_json::json!({
                            "image_base64": b64,
                            "content_type": content_type
                        })));
                    }
                    let text = resp.text().await.unwrap_or_else(|_| "{}".to_string());
                    let raw = serde_json::from_str::<serde_json::Value>(&text).unwrap_or_else(|_| {
                        serde_json::json!({ "raw_text": text })
                    });
                    return Ok((attempt, raw));
                }
                let text = resp.text().await.unwrap_or_else(|_| "{}".to_string());
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

fn normalize_provider_payload(
    provider: &ProviderProxyConfig,
    url: &Url,
    payload: serde_json::Value,
) -> serde_json::Value {
    match provider.name {
        "banana" => normalize_banana_payload(url, payload),
        "claude" => normalize_claude_payload(provider, url, payload),
        "stitch" => normalize_stitch_payload(provider, url, payload),
        _ => payload,
    }
}

fn normalize_banana_payload(url: &Url, payload: serde_json::Value) -> serde_json::Value {
    let url_text = url.as_str().to_ascii_lowercase();

    // HuggingFace Inference API: {"inputs": "prompt", "parameters": {"seed": N}}
    if url_text.contains("huggingface.co") {
        if payload.get("inputs").is_some() {
            return payload;
        }
        let prompt = extract_prompt_text(&payload)
            .unwrap_or_else(|| serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()));
        let mut body = serde_json::json!({ "inputs": prompt });
        let seed = payload.get("seed").and_then(|v| v.as_u64())
            .or_else(|| payload.get("input").and_then(|i| i.get("seed")).and_then(|v| v.as_u64()));
        if let Some(seed) = seed {
            body["parameters"] = serde_json::json!({ "seed": seed });
        }
        return body;
    }

    // Google Generative Language API (Gemini)
    if url_text.contains("generativelanguage.googleapis.com")
        && url_text.contains(":generatecontent")
    {
        if payload.get("contents").is_some() {
            return payload;
        }
        let prompt = extract_prompt_text(&payload)
            .unwrap_or_else(|| serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()));
        return serde_json::json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [{ "text": prompt }]
                }
            ]
        });
    }

    payload
}

fn normalize_claude_payload(
    provider: &ProviderProxyConfig,
    url: &Url,
    payload: serde_json::Value,
) -> serde_json::Value {
    let url_text = url.as_str().to_ascii_lowercase();

    // OpenAI Chat Completions
    if url_text.contains("api.openai.com") && url_text.contains("/v1/chat/completions") {
        if payload.get("messages").is_some() && payload.get("model").is_some() {
            return payload;
        }
        // Codegen: build from spec_output → generate real HTML
        let spec_content = payload.pointer("/input/spec_output")
            .or_else(|| payload.pointer("/input/codegen_output"));
        if let Some(spec) = spec_content {
            let spec_text = match spec {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string_pretty(other).unwrap_or_default(),
            };
            // Also extract OpenAI response content if it's nested
            let spec_str = if let Some(content) = spec.pointer("/choices/0/message/content")
                .and_then(|v| v.as_str()) {
                content.to_string()
            } else {
                spec_text
            };
            return serde_json::json!({
                "model": provider.default_model.as_deref().unwrap_or("gpt-4o"),
                "max_tokens": 8192,
                "messages": [
                    {
                        "role": "system",
                        "content": "You are an expert frontend developer. Generate a complete, beautiful, production-ready single-page HTML application with inline CSS and JavaScript. Use a modern design with gradients, shadows, and smooth animations. Return ONLY the raw HTML code — no markdown, no code blocks, no explanation."
                    },
                    {
                        "role": "user",
                        "content": format!("Build a stunning landing page based on this UI spec:\n\n{}", spec_str)
                    }
                ]
            });
        }
        let prompt = extract_prompt_text(&payload)
            .unwrap_or_else(|| serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()));
        return serde_json::json!({
            "model": provider.default_model.as_deref().unwrap_or("gpt-4o"),
            "max_tokens": provider.default_max_tokens.unwrap_or(4096),
            "messages": [{ "role": "user", "content": prompt }]
        });
    }

    // Anthropic Messages API
    if url_text.contains("api.anthropic.com") && url_text.contains("/v1/messages") {
        if payload.get("messages").is_some() && payload.get("model").is_some() {
            return payload;
        }
        let prompt = extract_prompt_text(&payload)
            .unwrap_or_else(|| serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()));
        return serde_json::json!({
            "model": provider.default_model.as_deref().unwrap_or("claude-3-5-sonnet-latest"),
            "max_tokens": provider.default_max_tokens.unwrap_or(1024),
            "messages": [{ "role": "user", "content": prompt }]
        });
    }

    payload
}

fn normalize_stitch_payload(
    provider: &ProviderProxyConfig,
    url: &Url,
    payload: serde_json::Value,
) -> serde_json::Value {
    let url_text = url.as_str().to_ascii_lowercase();

    // OpenAI Chat Completions — fallback khi chưa có Stitch
    if url_text.contains("api.openai.com") && url_text.contains("/v1/chat/completions") {
        if payload.get("messages").is_some() && payload.get("model").is_some() {
            return payload;
        }
        let image_b64 = payload.pointer("/input/design_output/image_base64")
            .and_then(|v| v.as_str());
        if let Some(b64) = image_b64 {
            let content_type = payload.pointer("/input/design_output/content_type")
                .and_then(|v| v.as_str())
                .unwrap_or("image/jpeg");
            return serde_json::json!({
                "model": provider.default_model.as_deref().unwrap_or("gpt-4o"),
                "max_tokens": provider.default_max_tokens.unwrap_or(4096),
                "messages": [{
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": { "url": format!("data:{};base64,{}", content_type, b64) }
                        },
                        {
                            "type": "text",
                            "text": "Analyze this UI mockup image and extract a structured spec as JSON with fields: component_list, color_palette (hex values), layout_description, typography (font families, sizes), key_sections. Be detailed and precise."
                        }
                    ]
                }]
            });
        }
        let input = serde_json::to_string(&payload).unwrap_or_default();
        return serde_json::json!({
            "model": provider.default_model.as_deref().unwrap_or("gpt-4o"),
            "max_tokens": provider.default_max_tokens.unwrap_or(4096),
            "messages": [{
                "role": "user",
                "content": format!(
                    "You are a UI spec extractor. Given this design output, extract a structured spec with: component list, color tokens, layout description, and typography. Output as JSON.\n\nDesign output:\n{}",
                    input
                )
            }]
        });
    }

    // Stitch MCP (stitch.googleapis.com/mcp or proxy)
    if url_text.contains("/mcp") {
        if payload.get("jsonrpc").is_some() && payload.get("method").is_some() {
            return payload;
        }
        let tool_name = provider.mcp_tool_name.as_deref().unwrap_or("extract_design_context");
        // Extract screen_id and project_id from input if available
        let arguments = if let Some(input) = payload.get("input") {
            let mut args = serde_json::Map::new();
            if let Some(project_id) = input.get("project_id").and_then(|v| v.as_str()) {
                args.insert("project_id".to_string(), serde_json::json!(project_id));
            }
            if let Some(screen_id) = input.get("screen_id").and_then(|v| v.as_str()) {
                args.insert("screen_id".to_string(), serde_json::json!(screen_id));
            }
            if let Some(prompt) = input.get("prompt").and_then(|v| v.as_str()) {
                args.insert("prompt".to_string(), serde_json::json!(prompt));
            }
            if let Some(model_id) = input.get("model_id").and_then(|v| v.as_str()) {
                args.insert("model_id".to_string(), serde_json::json!(model_id));
            }
            if let Some(device_type) = input.get("device_type").and_then(|v| v.as_str()) {
                args.insert("device_type".to_string(), serde_json::json!(device_type));
            }
            if args.is_empty() {
                input.clone()
            } else {
                serde_json::Value::Object(args)
            }
        } else {
            payload
        };
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "1".to_string());
        return serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });
    }

    // Gemini API fallback for spec extraction
    if url_text.contains("generativelanguage.googleapis.com") {
        let input_text = if let Some(input) = payload.get("input") {
            serde_json::to_string_pretty(input).unwrap_or_default()
        } else {
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        };
        return serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [{
                    "text": format!(
                        "You are a UI spec extractor. Extract a structured Design DNA spec from this design output as JSON with fields: component_list, color_palette (Tailwind classes + hex), layout_description, typography (font families, sizes), key_sections.\n\nDesign output:\n{}",
                        input_text
                    )
                }]
            }]
        });
    }

    payload
}

fn normalize_stitch_tool_call(
    _provider: &ProviderProxyConfig,
    tool_name: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    // If already a valid JSON-RPC request, pass through
    if payload.get("jsonrpc").is_some() && payload.get("method").is_some() {
        return payload;
    }
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "1".to_string());
    // Extract arguments from input wrapper if present
    let arguments = if let Some(input) = payload.get("input") {
        input.clone()
    } else {
        payload
    };
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    })
}

fn extract_prompt_text(payload: &serde_json::Value) -> Option<String> {
    if let Some(v) = payload
        .get("input")
        .and_then(|v| v.get("prompt"))
        .and_then(|v| v.as_str())
    {
        return Some(v.to_string());
    }
    if let Some(v) = payload.get("prompt").and_then(|v| v.as_str()) {
        return Some(v.to_string());
    }
    if let Some(v) = payload
        .get("input")
        .and_then(|v| v.get("design_output"))
        .and_then(|v| v.as_str())
    {
        return Some(v.to_string());
    }
    payload
        .get("design_output")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
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

fn parse_extra_headers(var_name: &str) -> Vec<(String, String)> {
    let raw = std::env::var(var_name).ok();
    let Some(raw) = raw else {
        return Vec::new();
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }
    let parsed = serde_json::from_str::<serde_json::Value>(raw);
    let Ok(serde_json::Value::Object(map)) = parsed else {
        warn!(env = var_name, "invalid extra headers json, expected object");
        return Vec::new();
    };
    map.into_iter()
        .filter_map(|(k, v)| v.as_str().map(|vv| (k, vv.to_string())))
        .collect()
}
