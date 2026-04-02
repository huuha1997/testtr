use redis::{AsyncCommands, RedisResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueJob {
    pub job_id: Uuid,
    pub run_id: Uuid,
    pub step: String,
    pub attempt: i32,
    pub payload_json: String,
}

pub async fn enqueue(
    conn: &mut redis::aio::MultiplexedConnection,
    stream_key: &str,
    job: &QueueJob,
) -> RedisResult<String> {
    conn.xadd(
        stream_key,
        "*",
        &[
            ("job_id", job.job_id.to_string()),
            ("run_id", job.run_id.to_string()),
            ("step", job.step.clone()),
            ("attempt", job.attempt.to_string()),
            ("payload_json", job.payload_json.clone()),
        ],
    )
    .await
}

pub async fn ack(
    conn: &mut redis::aio::MultiplexedConnection,
    stream_key: &str,
    group: &str,
    message_id: &str,
) -> RedisResult<i32> {
    conn.xack(stream_key, group, &[message_id]).await
}

pub async fn acquire_idempotency_lock(
    conn: &mut redis::aio::MultiplexedConnection,
    lock_key: &str,
    ttl_seconds: usize,
) -> RedisResult<bool> {
    let result: Option<String> = redis::cmd("SET")
        .arg(lock_key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(ttl_seconds)
        .query_async(conn)
        .await?;
    Ok(result.is_some())
}
