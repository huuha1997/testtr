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

const STREAM_KEY: &str = "q.spec";
const NEXT_STREAM_KEY: &str = "q.codegen";
const DLQ_STREAM_KEY: &str = "q.spec.dlq";
const GROUP: &str = "cg.spec";
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
    let consumer_name = std::env::var("SPEC_CONSUMER_NAME")
        .unwrap_or_else(|_| format!("spec-worker-{}", Uuid::new_v4()));
    let mcp_gateway_base_url = std::env::var("MCP_GATEWAY_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string())
        .trim_end_matches('/')
        .to_string();
    let mcp_http_timeout_seconds: u64 = std::env::var("MCP_HTTP_TIMEOUT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

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
    info!(consumer = %consumer_name, "spec worker started");

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
    match process_job(db, state, &job, conn).await {
        Ok(()) => {
            ack(conn, STREAM_KEY, GROUP, &id.id).await?;
        }
        Err(err) => {
            warn!(error = %err, run_id = %job.run_id, "spec processing failed");
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
    conn: &mut redis::aio::MultiplexedConnection,
) -> anyhow::Result<()> {
    if job.payload_json.contains("\"force_fail\":true") {
        return Err(anyhow::anyhow!("forced failure"));
    }
    let input_payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap_or_default();
    let output = call_gateway_spec(state, job.run_id, input_payload).await?;

    sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
        .bind(job.run_id)
        .bind(RunStatus::SpecGenerating.as_str())
        .execute(db)
        .await?;
    sqlx::query(
        "INSERT INTO run_steps (run_id, step_key, status, detail) VALUES ($1, $2, $3, $4)
         ON CONFLICT (run_id, step_key)
         DO UPDATE SET status = EXCLUDED.status, detail = EXCLUDED.detail, updated_at = now()",
    )
    .bind(job.run_id)
    .bind("spec_generation")
    .bind("completed")
    .bind(Some(format!(
        "processed_at={},provider_output={}",
        Utc::now().to_rfc3339(),
        compact_json(&output)
    )))
    .execute(db)
    .await?;
    enqueue(
        conn,
        NEXT_STREAM_KEY,
        &QueueJob {
            job_id: Uuid::new_v4(),
            run_id: job.run_id,
            step: "codegen".to_string(),
            attempt: 1,
            payload_json: serde_json::json!({
                "source": "spec_worker",
                "spec_output": output
            })
            .to_string(),
        },
    )
    .await?;
    info!(run_id = %job.run_id, "spec job processed");
    Ok(())
}

async fn call_gateway_spec(
    state: &WorkerState,
    run_id: Uuid,
    input_payload: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}/mcp/spec/extract", state.mcp_gateway_base_url);
    let mut req = state.mcp_gateway_http.post(url).json(&GatewayRequest {
        payload: serde_json::json!({
            "run_id": run_id,
            "input": input_payload
        }),
    });
    if let Some(key) = state.mcp_internal_api_key.as_ref() {
        req = req.header("x-internal-api-key", key);
    }
    let resp = req.send().await.context("mcp-gateway request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "mcp-gateway spec failed: http {}: {}",
            status.as_u16(),
            body
        ));
    }
    let out: GatewayResponse = resp
        .json()
        .await
        .context("invalid mcp-gateway spec response")?;
    Ok(out.raw)
}

fn compact_json(value: &serde_json::Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if text.len() <= 400 {
        text
    } else {
        format!("{}...", &text[..400])
    }
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
