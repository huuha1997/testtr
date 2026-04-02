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

const STREAM_KEY: &str = "q.ci";
const NEXT_STREAM_KEY: &str = "q.deploy";
const DLQ_STREAM_KEY: &str = "q.ci.dlq";
const GROUP: &str = "cg.ci";
const MAX_ATTEMPTS: i32 = 3;
const IDEMPOTENCY_LOCK_TTL_SECONDS: usize = 1800;
const MAX_REPAIR_ITERATIONS: i64 = 2;
const QUALITY_GATES: [&str; 6] = ["lint", "typecheck", "build", "e2e", "visual", "a11y"];
const DEFAULT_DEPLOY_PROJECT_NAME: &str = "agentic-preview";

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
    let consumer_name =
        std::env::var("CI_CONSUMER_NAME").unwrap_or_else(|_| format!("ci-worker-{}", Uuid::new_v4()));

    let db = PgPool::connect(&database_url).await?;
    let redis = redis::Client::open(redis_url)?;
    let mut conn = redis.get_multiplexed_async_connection().await?;

    ensure_group(&mut conn).await?;
    info!(consumer = %consumer_name, "ci worker started");

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
    match process_job(db, &job, conn).await {
        Ok(()) => {
            ack(conn, STREAM_KEY, GROUP, &id.id).await?;
        }
        Err(err) => {
            warn!(error = %err, run_id = %job.run_id, "ci processing failed");
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
    job: &QueueJob,
    conn: &mut redis::aio::MultiplexedConnection,
) -> anyhow::Result<()> {
    if job.payload_json.contains("\"force_fail\":true") {
        return Err(anyhow::anyhow!("forced failure"));
    }
    sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
        .bind(job.run_id)
        .bind(RunStatus::CiRunning.as_str())
        .execute(db)
        .await?;
    sqlx::query(
        "INSERT INTO run_steps (run_id, step_key, status, detail) VALUES ($1, $2, $3, $4)
         ON CONFLICT (run_id, step_key)
         DO UPDATE SET status = EXCLUDED.status, detail = EXCLUDED.detail, updated_at = now()",
    )
    .bind(job.run_id)
    .bind("ci")
    .bind("completed")
    .bind(Some(format!("processed_at={}", Utc::now().to_rfc3339())))
    .execute(db)
    .await?;
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap_or_default();
    let repair_iteration = payload
        .get("repair_iteration")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let failing_gates: Vec<String> = payload
        .get("force_fail_gates")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for gate in QUALITY_GATES {
        let passed = !failing_gates.iter().any(|f| f == gate);
        upsert_step(
            db,
            job.run_id,
            format!("ci_gate_{}", gate),
            if passed { "passed" } else { "failed" }.to_string(),
            Some(format!("iteration={}", repair_iteration)),
        )
        .await?;
    }
    if !failing_gates.is_empty() {
        if repair_iteration < MAX_REPAIR_ITERATIONS {
            sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
                .bind(job.run_id)
                .bind(RunStatus::FailedRetryable.as_str())
                .execute(db)
                .await?;
            upsert_step(
                db,
                job.run_id,
                "self_heal".to_string(),
                "retrying".to_string(),
                Some(format!(
                    "failed_gates={},next_iteration={}",
                    failing_gates.join(","),
                    repair_iteration + 1
                )),
            )
            .await?;
            enqueue(
                conn,
                "q.codegen",
                &QueueJob {
                    job_id: Uuid::new_v4(),
                    run_id: job.run_id,
                    step: "codegen_repair".to_string(),
                    attempt: 1,
                    payload_json: serde_json::json!({
                        "source": "ci_worker",
                        "repair_iteration": repair_iteration + 1,
                        "force_fail_gates": failing_gates
                    })
                    .to_string(),
                },
            )
            .await?;
            return Ok(());
        }
        sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
            .bind(job.run_id)
            .bind(RunStatus::FailedFinal.as_str())
            .execute(db)
            .await?;
        upsert_step(
            db,
            job.run_id,
            "self_heal".to_string(),
            "failed_final".to_string(),
            Some(format!(
                "max_iterations_reached={},failed_gates={}",
                MAX_REPAIR_ITERATIONS,
                failing_gates.join(",")
            )),
        )
        .await?;
        info!(run_id = %job.run_id, "ci gates failed permanently");
        return Ok(());
    }
    enqueue(
        conn,
        NEXT_STREAM_KEY,
        &QueueJob {
            job_id: Uuid::new_v4(),
            run_id: job.run_id,
            step: "deploy".to_string(),
            attempt: 1,
            payload_json: serde_json::json!({
                "source": "ci_worker",
                "repair_iteration": repair_iteration,
                "deployment": build_deployment_payload(job.run_id, repair_iteration)
            })
            .to_string(),
        },
    )
    .await?;
    info!(run_id = %job.run_id, "ci job processed");
    Ok(())
}

fn build_deployment_payload(run_id: Uuid, repair_iteration: i64) -> serde_json::Value {
    let project_name = std::env::var("CI_DEPLOY_VERCEL_PROJECT_NAME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_DEPLOY_PROJECT_NAME.to_string());
    let run_id_str = run_id.to_string();
    let run_short = &run_id_str[..8];
    let now = Utc::now().to_rfc3339();
    serde_json::json!({
        "name": format!("{}-{}", project_name, run_short),
        "files": [
            {
                "file": "index.html",
                "data": format!(
                    "<!doctype html><html><head><meta charset=\"utf-8\"><title>Agentic Preview</title></head><body><h1>Agentic Preview</h1><p>run_id={}</p><p>repair_iteration={}</p><p>generated_at={}</p></body></html>",
                    run_id,
                    repair_iteration,
                    now
                )
            }
        ],
        "projectSettings": {
            "framework": null
        }
    })
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
