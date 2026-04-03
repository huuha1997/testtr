use std::{collections::HashMap, time::Duration};

use anyhow::Context;
use chrono::Utc;
use contracts::RunStatus;
use queue::{QueueJob, ack, acquire_idempotency_lock, enqueue};
use redis::{
    AsyncCommands, Value,
    streams::{StreamId, StreamReadOptions, StreamReadReply},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

const STREAM_KEY: &str = "q.stitch";
const DLQ_STREAM_KEY: &str = "q.stitch.dlq";
const GROUP: &str = "cg.stitch";
const MAX_ATTEMPTS: i32 = 3;
const IDEMPOTENCY_LOCK_TTL_SECONDS: usize = 1800;

#[derive(Clone)]
struct WorkerState {
    mcp_gateway_base_url: String,
    mcp_gateway_http: reqwest::Client,
    mcp_internal_api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct GatewayRequest {
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GatewayResponse {
    raw: serde_json::Value,
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
    let consumer_name = std::env::var("STITCH_CONSUMER_NAME")
        .unwrap_or_else(|_| format!("stitch-worker-{}", Uuid::new_v4()));
    let mcp_gateway_base_url = std::env::var("MCP_GATEWAY_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string())
        .trim_end_matches('/')
        .to_string();
    let mcp_http_timeout_seconds: u64 = std::env::var("MCP_HTTP_TIMEOUT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let state = WorkerState {
        mcp_gateway_base_url,
        mcp_gateway_http: reqwest::Client::builder()
            .timeout(Duration::from_secs(mcp_http_timeout_seconds))
            .build()?,
        mcp_internal_api_key: std::env::var("MCP_INTERNAL_API_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty()),
    };

    let db = PgPool::connect(&database_url).await?;
    let redis = redis::Client::open(redis_url)?;
    let mut conn = redis.get_multiplexed_async_connection().await?;

    ensure_group(&mut conn).await?;
    info!(consumer = %consumer_name, "stitch worker started");

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
            warn!(error = %err, run_id = %job.run_id, "stitch processing failed");
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

async fn process_job(
    db: &PgPool,
    state: &WorkerState,
    job: &QueueJob,
) -> anyhow::Result<()> {
    if job.payload_json.contains("\"force_fail\":true") {
        return Err(anyhow::anyhow!("forced failure"));
    }

    // Update status → StitchGenerating
    sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
        .bind(job.run_id)
        .bind(RunStatus::StitchGenerating.as_str())
        .execute(db)
        .await?;

    let input: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap_or_default();
    let prompt = input.get("prompt").and_then(|v| v.as_str()).unwrap_or("A modern landing page");
    let selected_mockup_id = input.get("selected_mockup_id").and_then(|v| v.as_str()).unwrap_or("A");

    // Extract the selected mockup image from banana output
    let mockup_detail_str = input.get("mockup_detail").and_then(|v| v.as_str()).unwrap_or("{}");
    let mockup_detail: serde_json::Value = serde_json::from_str(mockup_detail_str).unwrap_or_default();
    let selected_mockup = mockup_detail
        .pointer(&format!("/mockups/{}", selected_mockup_id))
        .cloned()
        .unwrap_or_default();

    // If mockup has image_base64, use Gemini Vision to describe it for Stitch
    let image_b64 = selected_mockup.get("image_base64").and_then(|v| v.as_str());
    let mockup_description = if let Some(b64) = image_b64 {
        info!(run_id = %job.run_id, "analyzing mockup image with Gemini Vision");
        match describe_image_with_gemini(state, b64).await {
            Ok(desc) => {
                info!(run_id = %job.run_id, desc_len = desc.len(), "mockup image described");
                desc
            }
            Err(e) => {
                warn!(run_id = %job.run_id, error = %e, "failed to describe mockup, using prompt only");
                String::new()
            }
        }
    } else {
        // Try text description if available (Gemini text output)
        selected_mockup
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(|v| v.as_str())
            .or_else(|| selected_mockup.get("raw_text").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string()
    };

    let stitch_prompt = if mockup_description.is_empty() {
        prompt.to_string()
    } else {
        format!(
            "Generate a UI screen that matches this mockup design.\n\n\
             User brief: {}\n\n\
             Mockup analysis:\n{}",
            prompt, mockup_description
        )
    };

    // Call Stitch generate_screen_from_text via stitch-mcp CLI
    let stitch_project_id = std::env::var("STITCH_PROJECT_ID")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "635644491768772364".to_string());
    let tool_args = serde_json::json!({
        "project_id": stitch_project_id,
        "prompt": stitch_prompt,
        "model_id": "GEMINI_3_FLASH",
        "device_type": "DESKTOP"
    });

    let output = call_stitch_cli("generate_screen_from_text", &tool_args).await?;

    // Build Stitch web URL from output
    let stitch_project_id_val = output.get("projectId")
        .or_else(|| output.pointer("/project_id"))
        .and_then(|v| v.as_str())
        .unwrap_or(&stitch_project_id);
    let session_id = output.get("sessionId")
        .or_else(|| output.pointer("/session_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let stitch_url = if let Some(ref sid) = session_id {
        format!("https://stitch.withgoogle.com/project/{}/session/{}", stitch_project_id_val, sid)
    } else {
        format!("https://stitch.withgoogle.com/project/{}", stitch_project_id_val)
    };
    info!(run_id = %job.run_id, %stitch_url, "stitch design URL");

    let detail = serde_json::to_string(&serde_json::json!({
        "processed_at": Utc::now().to_rfc3339(),
        "selected_mockup_id": selected_mockup_id,
        "stitch_url": stitch_url,
        "stitch_output": output
    }))
    .unwrap_or_default();

    // Update status → StitchReady (waiting for user approval)
    sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
        .bind(job.run_id)
        .bind(RunStatus::StitchReady.as_str())
        .execute(db)
        .await?;
    sqlx::query(
        "INSERT INTO run_steps (run_id, step_key, status, detail) VALUES ($1, $2, $3, $4)
         ON CONFLICT (run_id, step_key)
         DO UPDATE SET status = EXCLUDED.status, detail = EXCLUDED.detail, updated_at = now()",
    )
    .bind(job.run_id)
    .bind("stitch_generation")
    .bind("completed")
    .bind(Some(detail))
    .execute(db)
    .await?;

    // Do NOT auto-enqueue — wait for user to approve Stitch design
    info!(run_id = %job.run_id, "stitch design generated — waiting for approval");
    Ok(())
}

async fn describe_image_with_gemini(
    state: &WorkerState,
    image_b64: &str,
) -> anyhow::Result<String> {
    let gemini_api_key = std::env::var("GEMINI_API_KEY")
        .context("GEMINI_API_KEY not set — needed for mockup image analysis")?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}",
        gemini_api_key
    );

    // Detect content type from base64 header
    let content_type = if image_b64.starts_with("/9j/") {
        "image/jpeg"
    } else if image_b64.starts_with("iVBOR") {
        "image/png"
    } else {
        "image/jpeg"
    };

    let body = serde_json::json!({
        "contents": [{
            "parts": [
                {
                    "inline_data": {
                        "mime_type": content_type,
                        "data": image_b64
                    }
                },
                {
                    "text": "Analyze this UI mockup image in detail. Describe:\n\
                    1. Overall layout structure and sections\n\
                    2. Color palette (exact hex values if possible)\n\
                    3. Typography styles\n\
                    4. All UI components and their positions\n\
                    5. Visual hierarchy and spacing\n\
                    6. Any icons, images, or decorative elements\n\
                    Be very detailed so a designer can recreate this exactly."
                }
            ]
        }]
    });

    let resp = state.mcp_gateway_http
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Gemini Vision request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Gemini Vision failed: {} {}", status, &text[..text.len().min(200)]));
    }

    let result: serde_json::Value = resp.json().await.context("invalid Gemini response")?;
    let text = result
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("Unable to analyze image")
        .to_string();

    Ok(text)
}

async fn call_stitch_cli(
    tool_name: &str,
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let args_json = serde_json::to_string(args)?;
    info!(tool = tool_name, "calling stitch-mcp tool");

    // Get OAuth access token from gcloud ADC
    let gcloud_path = format!(
        "{}/.stitch-mcp/google-cloud-sdk/bin/gcloud",
        std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
    );
    let token_output = tokio::process::Command::new(&gcloud_path)
        .args(["auth", "application-default", "print-access-token"])
        .output()
        .await
        .context("failed to get access token from gcloud")?;
    let access_token = String::from_utf8_lossy(&token_output.stdout).trim().to_string();
    if access_token.is_empty() {
        return Err(anyhow::anyhow!("gcloud access token is empty — run: gcloud auth application-default login"));
    }

    let project_id = std::env::var("GOOGLE_CLOUD_PROJECT")
        .unwrap_or_else(|_| "gen-lang-client-0801594079".to_string());

    let output = tokio::process::Command::new("npx")
        .args(["@_davideast/stitch-mcp", "tool", tool_name, "-d", &args_json])
        .env("STITCH_ACCESS_TOKEN", &access_token)
        .env("GOOGLE_CLOUD_PROJECT", &project_id)
        .output()
        .await
        .context("stitch-mcp tool command failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow::anyhow!(
            "stitch-mcp tool {} failed (exit {}): {} {}",
            tool_name, output.status, stderr, stdout
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| serde_json::json!({ "raw_text": stdout.trim() }));
    Ok(parsed)
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
