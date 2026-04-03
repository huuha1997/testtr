use std::{collections::HashMap, time::Duration};

use anyhow::Context;
use contracts::RunStatus;
use queue::{QueueJob, ack, acquire_idempotency_lock, enqueue};
use redis::{
    AsyncCommands, Value,
    streams::{StreamId, StreamReadOptions, StreamReadReply},
};
use serde_json::json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

const STREAM_KEY: &str = "q.deploy";
const DLQ_STREAM_KEY: &str = "q.deploy.dlq";
const GROUP: &str = "cg.deploy";
const MAX_ATTEMPTS: i32 = 3;
const IDEMPOTENCY_LOCK_TTL_SECONDS: usize = 1800;

#[derive(Clone)]
struct WorkerState {
    mcp_gateway_base_url: String,
    mcp_gateway_http: reqwest::Client,
    mcp_internal_api_key: Option<String>,
    deploy_github_owner: Option<String>,
    deploy_github_repo: Option<String>,
    deploy_github_base_branch: String,
    deploy_github_head_prefix: String,
    deploy_vercel_team_id: Option<String>,
    deploy_vercel_slug: Option<String>,
    deploy_vercel_project_name: String,
}

#[derive(Debug, Serialize)]
struct GatewayGithubCreatePrRequest {
    owner: String,
    repo: String,
    head: String,
    base: String,
    title: String,
    body: String,
    draft: bool,
}

#[derive(Debug, Deserialize)]
struct GatewayGithubCreatePrResponse {
    number: i64,
    html_url: Option<String>,
    state: Option<String>,
}

#[derive(Debug, Serialize)]
struct GatewayVercelDeployRequest {
    team_id: Option<String>,
    slug: Option<String>,
    deployment: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GatewayVercelDeployResponse {
    deployment_id: String,
    ready_state: Option<String>,
    deployment_url: Option<String>,
    inspector_url: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/agentic".to_string());
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let consumer_name = std::env::var("DEPLOY_CONSUMER_NAME")
        .unwrap_or_else(|_| format!("deploy-worker-{}", Uuid::new_v4()));
    let mcp_gateway_base_url = std::env::var("MCP_GATEWAY_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string())
        .trim_end_matches('/')
        .to_string();
    let deploy_http_timeout_seconds: u64 = std::env::var("DEPLOY_HTTP_TIMEOUT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let deploy_vercel_team_id = std::env::var("DEPLOY_VERCEL_TEAM_ID")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let deploy_vercel_slug = std::env::var("DEPLOY_VERCEL_SLUG")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let deploy_vercel_project_name = std::env::var("DEPLOY_VERCEL_PROJECT_NAME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "agentic-preview".to_string());
    let deploy_github_owner = std::env::var("DEPLOY_GITHUB_OWNER")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let deploy_github_repo = std::env::var("DEPLOY_GITHUB_REPO")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let deploy_github_base_branch = std::env::var("DEPLOY_GITHUB_BASE_BRANCH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());
    let deploy_github_head_prefix = std::env::var("DEPLOY_GITHUB_HEAD_PREFIX")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "preview/run-".to_string());

    let state = WorkerState {
        mcp_gateway_base_url,
        mcp_gateway_http: reqwest::Client::builder()
            .timeout(Duration::from_secs(deploy_http_timeout_seconds))
            .build()?,
        mcp_internal_api_key: std::env::var("MCP_INTERNAL_API_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty()),
        deploy_github_owner,
        deploy_github_repo,
        deploy_github_base_branch,
        deploy_github_head_prefix,
        deploy_vercel_team_id,
        deploy_vercel_slug,
        deploy_vercel_project_name,
    };

    let db = PgPool::connect(&database_url).await?;
    let redis = redis::Client::open(redis_url)?;
    let mut conn = redis.get_multiplexed_async_connection().await?;

    ensure_group(&mut conn).await?;
    info!(consumer = %consumer_name, "deploy worker started");

    loop {
        let opts = StreamReadOptions::default()
            .group(GROUP, &consumer_name)
            .count(10)
            .block(5000);
        let reply: redis::RedisResult<StreamReadReply> =
            conn.xread_options(&[STREAM_KEY], &[">"], &opts).await;
        let reply = match reply {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "xreadgroup failed");
                continue;
            }
        };
        for key in reply.keys {
            for id in key.ids {
                if let Err(err) = handle_message(&db, &state, &mut conn, id).await {
                    error!(error = %err, "message handling failed");
                }
            }
        }
    }
}

async fn ensure_group(conn: &mut redis::aio::MultiplexedConnection) -> anyhow::Result<()> {
    let result: redis::RedisResult<String> = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(STREAM_KEY)
        .arg(GROUP)
        .arg("$")
        .arg("MKSTREAM")
        .query_async(conn)
        .await;
    if let Err(err) = result {
        let msg = err.to_string();
        if !msg.contains("BUSYGROUP") {
            return Err(anyhow::anyhow!(msg));
        }
    }
    Ok(())
}

async fn handle_message(
    db: &PgPool,
    state: &WorkerState,
    conn: &mut redis::aio::MultiplexedConnection,
    id: StreamId,
) -> anyhow::Result<()> {
    let job = parse_job(&id.map).context("parse queue job failed")?;
    let lock_key = format!("idem:{}:{}:{}", STREAM_KEY, job.job_id, job.attempt);
    let acquired =
        acquire_idempotency_lock(conn, &lock_key, IDEMPOTENCY_LOCK_TTL_SECONDS).await?;
    if !acquired {
        info!(run_id = %job.run_id, job_id = %job.job_id, "duplicate job skipped");
        ack(conn, STREAM_KEY, GROUP, &id.id).await?;
        return Ok(());
    }
    match process_job(db, state, &job).await {
        Ok(()) => {
            ack(conn, STREAM_KEY, GROUP, &id.id).await?;
        }
        Err(err) => {
            warn!(error = %err, run_id = %job.run_id, "deploy processing failed");
            if job.attempt >= MAX_ATTEMPTS {
                enqueue(
                    conn,
                    DLQ_STREAM_KEY,
                    &QueueJob {
                        attempt: job.attempt + 1,
                        ..job.clone()
                    },
                )
                .await?;
                ack(conn, STREAM_KEY, GROUP, &id.id).await?;
            } else {
                enqueue(
                    conn,
                    STREAM_KEY,
                    &QueueJob {
                        attempt: job.attempt + 1,
                        ..job.clone()
                    },
                )
                .await?;
                ack(conn, STREAM_KEY, GROUP, &id.id).await?;
            }
        }
    }
    Ok(())
}

async fn process_job(db: &PgPool, state: &WorkerState, job: &QueueJob) -> anyhow::Result<()> {
    if job.payload_json.contains("\"force_fail\":true") {
        return Err(anyhow::anyhow!("forced failure"));
    }

    sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
        .bind(job.run_id)
        .bind(RunStatus::PrReady.as_str())
        .execute(db)
        .await?;
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap_or_default();
    let pr_result = match maybe_create_pull_request(state, job.run_id, &payload).await {
        Ok(r) => r,
        Err(e) => {
            warn!(run_id = %job.run_id, "PR creation failed (non-fatal): {}", e);
            format!("skipped: {}", e)
        }
    };
    upsert_step(
        db,
        job.run_id,
        "pr_create".to_string(),
        "completed".to_string(),
        Some(pr_result),
    )
    .await?;

    // Deploy via Vercel CLI (more reliable than API token)
    let deployed = deploy_via_vercel_cli(job.run_id, &job.payload_json).await?;

    sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
        .bind(job.run_id)
        .bind(RunStatus::PreviewDeployed.as_str())
        .execute(db)
        .await?;
    upsert_step(
        db,
        job.run_id,
        "preview_deploy".to_string(),
        "completed".to_string(),
        Some(format!(
            "preview_url={},inspect_url={}",
            deployed.0, deployed.1
        )),
    )
    .await?;

    sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
        .bind(job.run_id)
        .bind(RunStatus::AwaitingApproval.as_str())
        .execute(db)
        .await?;
    upsert_step(
        db,
        job.run_id,
        "deploy_approval".to_string(),
        "waiting".to_string(),
        Some("waiting for manual approval".to_string()),
    )
    .await?;
    info!(run_id = %job.run_id, "deploy job processed");
    Ok(())
}

fn build_gateway_deploy_request(
    state: &WorkerState,
    run_id: Uuid,
    payload_json: &str,
) -> anyhow::Result<GatewayVercelDeployRequest> {
    let payload: serde_json::Value = serde_json::from_str(payload_json).unwrap_or_default();
    let deployment = payload
        .get("deployment")
        .and_then(|d| d.as_object().map(|_| d.clone()))
        .unwrap_or_else(|| {
            // Default static deployment so end-to-end can run before git integration is complete.
            json!({
                "name": format!("{}-{}", state.deploy_vercel_project_name, &run_id.to_string()[..8]),
                "files": [
                    {
                        "file": "index.html",
                        "data": format!(
                            "<!doctype html><html><body><h1>Agentic Preview</h1><p>run_id={}</p></body></html>",
                            run_id
                        )
                    }
                ],
                "projectSettings": {
                    "framework": null
                }
            })
        });

    Ok(GatewayVercelDeployRequest {
        team_id: state.deploy_vercel_team_id.clone(),
        slug: state.deploy_vercel_slug.clone(),
        deployment,
    })
}

async fn trigger_vercel_preview(
    state: &WorkerState,
    payload: &GatewayVercelDeployRequest,
) -> anyhow::Result<GatewayVercelDeployResponse> {
    let url = format!("{}/mcp/deploy/vercel", state.mcp_gateway_base_url);
    let mut req = state.mcp_gateway_http.post(url).json(payload);
    if let Some(key) = state.mcp_internal_api_key.as_ref() {
        req = req.header("x-internal-api-key", key);
    }
    let response = req.send().await.context("mcp-gateway request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "mcp-gateway deploy failed: http {}: {}",
            status.as_u16(),
            body
        ));
    }

    response
        .json::<GatewayVercelDeployResponse>()
        .await
        .context("invalid mcp-gateway deploy response")
}

async fn maybe_create_pull_request(
    state: &WorkerState,
    run_id: Uuid,
    payload: &serde_json::Value,
) -> anyhow::Result<String> {
    let owner = payload
        .pointer("/pr/owner")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .or_else(|| state.deploy_github_owner.clone());
    let repo = payload
        .pointer("/pr/repo")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .or_else(|| state.deploy_github_repo.clone());
    let Some(owner) = owner else {
        return Ok("skipped: DEPLOY_GITHUB_OWNER not configured".to_string());
    };
    let Some(repo) = repo else {
        return Ok("skipped: DEPLOY_GITHUB_REPO not configured".to_string());
    };
    let run_short = &run_id.to_string()[..8];
    let head = payload
        .pointer("/pr/head")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| format!("{}{}", state.deploy_github_head_prefix, run_short));
    let base = payload
        .pointer("/pr/base")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| state.deploy_github_base_branch.clone());
    let title = payload
        .pointer("/pr/title")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| format!("Agentic preview for run {}", run_short));
    let body = payload
        .pointer("/pr/body")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| format!("Auto-generated from run {}", run_id));
    let draft = payload
        .pointer("/pr/draft")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    // Ensure head branch exists and has a commit with generated HTML
    let codegen_html = payload.pointer("/deployment/files/0/data")
        .and_then(|v| v.as_str())
        .or_else(|| payload.pointer("/codegen_output/choices/0/message/content").and_then(|v| v.as_str()))
        .unwrap_or("<!-- generated by agentic pipeline -->");
    if let Err(e) = ensure_branch_with_content(state, &owner, &repo, &head, &base, codegen_html, run_id).await {
        warn!("branch+commit creation failed (non-fatal): {}", e);
    }

    let req_payload = GatewayGithubCreatePrRequest {
        owner,
        repo,
        head,
        base,
        title,
        body,
        draft,
    };
    let url = format!("{}/mcp/repo/create-pr", state.mcp_gateway_base_url);
    let mut req = state.mcp_gateway_http.post(url).json(&req_payload);
    if let Some(key) = state.mcp_internal_api_key.as_ref() {
        req = req.header("x-internal-api-key", key);
    }
    let response = req.send().await.context("mcp-gateway request failed")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "mcp-gateway create-pr failed: http {}: {}",
            status.as_u16(),
            body
        ));
    }
    let out = response
        .json::<GatewayGithubCreatePrResponse>()
        .await
        .context("invalid mcp-gateway create-pr response")?;
    Ok(format!(
        "pr_number={},pr_state={},pr_url={}",
        out.number,
        out.state.unwrap_or_else(|| "unknown".to_string()),
        out.html_url.unwrap_or_else(|| "unknown".to_string())
    ))
}

/// Deploy using Vercel CLI instead of API (avoids token expiration issues)
async fn deploy_via_vercel_cli(
    run_id: Uuid,
    payload_json: &str,
) -> anyhow::Result<(String, String)> {
    let payload: serde_json::Value = serde_json::from_str(payload_json).unwrap_or_default();

    // Extract HTML from deployment payload or codegen_output
    let html = payload.pointer("/deployment/files/0/data")
        .and_then(|v| v.as_str())
        .or_else(|| payload.pointer("/codegen_output/choices/0/message/content").and_then(|v| v.as_str()))
        .unwrap_or("<!doctype html><html><body><h1>Agentic Preview</h1></body></html>");

    // Write to temp dir
    let run_short = &run_id.to_string()[..8];
    let deploy_dir = format!("/tmp/agentic-deploy-{}", run_short);
    tokio::fs::create_dir_all(&deploy_dir).await.context("mkdir failed")?;
    tokio::fs::write(format!("{}/index.html", deploy_dir), html).await.context("write html failed")?;

    info!(run_id = %run_id, dir = %deploy_dir, html_len = html.len(), "deploying via vercel cli");

    let output = tokio::process::Command::new("npx")
        .args(["vercel", "deploy", "--yes"])
        .current_dir(&deploy_dir)
        .output()
        .await
        .context("vercel deploy command failed")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(anyhow::anyhow!("vercel deploy failed: {} {}", stderr, stdout));
    }

    // Parse deployment URL from stdout (last line is usually the URL)
    let deploy_url = stdout.lines()
        .rev()
        .find(|l| l.starts_with("https://"))
        .unwrap_or("unknown")
        .trim()
        .to_string();

    // Try to find inspect URL
    let inspect_url = stderr.lines()
        .chain(stdout.lines())
        .find(|l| l.contains("vercel.com") && l.contains("Inspect"))
        .and_then(|l| l.split_whitespace().find(|w| w.starts_with("https://")))
        .unwrap_or("unknown")
        .to_string();

    info!(run_id = %run_id, %deploy_url, "vercel deploy complete");

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&deploy_dir).await;

    Ok((deploy_url, inspect_url))
}

async fn ensure_branch_with_content(
    state: &WorkerState,
    owner: &str,
    repo: &str,
    head: &str,
    base: &str,
    html_content: &str,
    run_id: Uuid,
) -> anyhow::Result<()> {
    let token = std::env::var("GITHUB_API_TOKEN")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .context("GITHUB_API_TOKEN not set")?;
    let gh = std::env::var("GITHUB_API_BASE")
        .unwrap_or_else(|_| "https://api.github.com".to_string());
    let headers = |req: reqwest::RequestBuilder| -> reqwest::RequestBuilder {
        req.bearer_auth(&token)
            .header("accept", "application/vnd.github+json")
            .header("user-agent", "agentic-deploy-worker")
    };

    // 1. Get SHA of base branch
    let resp = headers(state.mcp_gateway_http.get(
        format!("{}/repos/{}/{}/git/ref/heads/{}", gh, owner, repo, base)
    )).send().await.context("failed to get base branch")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("get base branch {}: {} {}", base, s, b));
    }
    let base_ref: serde_json::Value = resp.json().await?;
    let base_sha = base_ref.pointer("/object/sha").and_then(|v| v.as_str())
        .context("no sha in base ref")?
        .to_string();

    // 2. Create head branch (ignore if exists)
    let resp = headers(state.mcp_gateway_http.post(
        format!("{}/repos/{}/{}/git/refs", gh, owner, repo)
    )).json(&json!({
        "ref": format!("refs/heads/{}", head),
        "sha": &base_sha
    })).send().await.context("failed to create branch")?;
    if resp.status().is_success() {
        info!(branch = head, "created branch from {}", base);
    } else {
        let body = resp.text().await.unwrap_or_default();
        if !body.contains("Reference already exists") {
            return Err(anyhow::anyhow!("create branch failed: {}", body));
        }
        info!(branch = head, "branch already exists");
    }

    // 3. Create/update index.html on the head branch via Contents API
    // Check if file exists first to get its SHA (needed for update)
    let file_path = "index.html";
    let contents_url = format!("{}/repos/{}/{}/contents/{}", gh, owner, repo, file_path);
    let file_sha = match headers(state.mcp_gateway_http.get(&contents_url))
        .query(&[("ref", head)])
        .send().await
    {
        Ok(resp) if resp.status().is_success() => {
            resp.json::<serde_json::Value>().await.ok()
                .and_then(|v| v.get("sha").and_then(|s| s.as_str()).map(|s| s.to_string()))
        }
        _ => None,
    };

    let mut put_body = json!({
        "message": format!("chore: generated preview for run {}", &run_id.to_string()[..8]),
        "content": base64_encode(html_content),
        "branch": head
    });
    if let Some(sha) = file_sha {
        put_body["sha"] = json!(sha);
    }

    let resp = headers(state.mcp_gateway_http.put(&contents_url))
        .json(&put_body)
        .send().await
        .context("failed to commit file")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("commit file failed: {} {}", s, &b[..b.len().min(300)]));
    }
    info!(branch = head, file = file_path, "committed generated HTML");
    Ok(())
}

fn base64_encode(input: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(input.as_bytes())
}

fn parse_job(map: &HashMap<String, Value>) -> anyhow::Result<QueueJob> {
    let job_id = value_str(map, "job_id")?.parse()?;
    let run_id = value_str(map, "run_id")?.parse()?;
    let step = value_str(map, "step")?;
    let attempt = value_str(map, "attempt")?.parse::<i32>()?;
    let payload_json = value_str(map, "payload_json")?;
    Ok(QueueJob {
        job_id,
        run_id,
        step,
        attempt,
        payload_json,
    })
}

fn value_str(map: &HashMap<String, Value>, key: &str) -> anyhow::Result<String> {
    let value = map
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("missing field {}", key))?;
    match value {
        Value::SimpleString(v) => Ok(v.to_string()),
        Value::BulkString(v) => String::from_utf8(v.to_vec()).map_err(|e| anyhow::anyhow!(e)),
        Value::Int(v) => Ok(v.to_string()),
        _ => Err(anyhow::anyhow!("unsupported redis value for {}", key)),
    }
}

async fn upsert_step(
    db: &PgPool,
    run_id: Uuid,
    step_key: String,
    status: String,
    detail: Option<String>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO run_steps (run_id, step_key, status, detail) VALUES ($1, $2, $3, $4)
         ON CONFLICT (run_id, step_key)
         DO UPDATE SET status = EXCLUDED.status, detail = EXCLUDED.detail, updated_at = now()",
    )
    .bind(run_id)
    .bind(step_key)
    .bind(status)
    .bind(detail)
    .execute(db)
    .await?;
    Ok(())
}
