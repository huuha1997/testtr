use std::collections::HashMap;

use anyhow::Context;
use chrono::Utc;
use contracts::RunStatus;
use queue::{QueueJob, ack, acquire_idempotency_lock, enqueue};
use redis::{
    AsyncCommands, Value,
    streams::{StreamId, StreamReadOptions, StreamReadReply},
};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

const STREAM_KEY: &str = "q.deploy";
const DLQ_STREAM_KEY: &str = "q.deploy.dlq";
const GROUP: &str = "cg.deploy";
const MAX_ATTEMPTS: i32 = 3;
const IDEMPOTENCY_LOCK_TTL_SECONDS: usize = 1800;

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
                if let Err(err) = handle_message(&db, &mut conn, id).await {
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
    match process_job(db, &job).await {
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

async fn process_job(db: &PgPool, job: &QueueJob) -> anyhow::Result<()> {
    if job.payload_json.contains("\"force_fail\":true") {
        return Err(anyhow::anyhow!("forced failure"));
    }
    sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
        .bind(job.run_id)
        .bind(RunStatus::PrReady.as_str())
        .execute(db)
        .await?;
    upsert_step(
        db,
        job.run_id,
        "pr_create".to_string(),
        "completed".to_string(),
        Some(format!("pr_ready_at={}", Utc::now().to_rfc3339())),
    )
    .await?;
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
        Some(format!("preview_url=http://preview.local/{}", job.run_id)),
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
