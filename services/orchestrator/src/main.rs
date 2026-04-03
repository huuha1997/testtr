use std::{convert::Infallible, net::SocketAddr, str::FromStr, time::Duration};

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, rand_core::RngCore},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, sse::Event, sse::KeepAlive, sse::Sse},
    routing::{delete, get, post, put},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use contracts::{
    ApproveStitchRequest, Connection, ConnectionProvider, CreateRunRequest, CreateRunResponse,
    DeleteConnectionResponse, ListConnectionsResponse, ListRunStepsResponse,
    MetricsSummaryResponse, OAuthCallbackRequest, OAuthStartRequest, OAuthStartResponse,
    RefreshConnectionResponse, RejectDeployRequest, RevokeConnectionResponse, Run, RunStatus,
    RunStep, RunTimelineItem, RunTimelineResponse, SelectMockupRequest, SelectStackRequest,
    SseEvent, TransitionRunResponse, UpsertConnectionRequest, UpsertConnectionResponse,
};
use futures_util::stream::{self, Stream};
use queue::{QueueJob, enqueue};
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::time;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    redis: redis::Client,
    sse_heartbeat: Duration,
    encryption_key: [u8; 32],
    http: reqwest::Client,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let http_addr: SocketAddr = std::env::var("HTTP_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/agentic".to_string());
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let sse_heartbeat_seconds: u64 = std::env::var("SSE_HEARTBEAT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let encryption_key = read_encryption_key()?;

    let db = PgPool::connect(&database_url).await?;
    sqlx::migrate!().run(&db).await?;

    let redis = redis::Client::open(redis_url)?;

    let state = AppState {
        db,
        redis,
        sse_heartbeat: Duration::from_secs(sse_heartbeat_seconds),
        encryption_key,
        http: reqwest::Client::new(),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/runs", post(create_run))
        .route("/api/runs/{run_id}", get(get_run))
        .route("/api/runs/{run_id}/select-mockup", post(select_mockup))
        .route("/api/runs/{run_id}/approve-stitch", post(approve_stitch))
        .route("/api/runs/{run_id}/select-stack", post(select_stack))
        .route("/api/runs/{run_id}/steps", get(list_run_steps))
        .route("/api/runs/{run_id}/timeline", get(list_run_timeline))
        .route("/api/runs/{run_id}/events", get(run_events_sse))
        .route("/api/runs/{run_id}/approve-deploy", post(approve_deploy))
        .route("/api/runs/{run_id}/reject-deploy", post(reject_deploy))
        .route("/api/connections", get(list_connections))
        .route("/api/metrics/summary", get(metrics_summary))
        .route("/api/connections/{provider}", put(upsert_connection))
        .route("/api/connections/{provider}", delete(delete_connection))
        .route(
            "/api/connections/{provider}/refresh",
            post(refresh_connection_token),
        )
        .route(
            "/api/connections/{provider}/revoke",
            post(revoke_connection),
        )
        .route(
            "/api/connections/{provider}/oauth/start",
            post(oauth_start_connection),
        )
        .route(
            "/api/connections/{provider}/oauth/callback",
            post(oauth_callback_connection),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    info!(%http_addr, "orchestrator listening");
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    if let Err(err) = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        error!(error = %err, "db not ready");
        return (StatusCode::SERVICE_UNAVAILABLE, "db not ready");
    }

    match state.redis.get_multiplexed_async_connection().await {
        Ok(mut conn) => {
            let pong: redis::RedisResult<String> = redis::cmd("PING").query_async(&mut conn).await;
            if pong.is_err() {
                return (StatusCode::SERVICE_UNAVAILABLE, "redis not ready");
            }
        }
        Err(_) => return (StatusCode::SERVICE_UNAVAILABLE, "redis not ready"),
    }

    (StatusCode::OK, "ok")
}

async fn create_run(
    State(state): State<AppState>,
    Json(payload): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, (StatusCode, String)> {
    if payload.prompt.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is empty".to_string()));
    }

    let id = Uuid::new_v4();
    let status = RunStatus::MockupGenerating;
    let prompt = payload.prompt;

    let row = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<Utc>)>(
        "INSERT INTO runs (id, status, prompt) VALUES ($1, $2, $3) RETURNING id, status, created_at",
    )
    .bind(id)
    .bind(status.as_str())
    .bind(prompt.clone())
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    upsert_step(
        &state.db,
        id,
        "mockup_generation".to_string(),
        "queued".to_string(),
        Some("banana/stitch/claude pipeline bootstrapped".to_string()),
    )
    .await?;

    let mut conn = state
        .redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let payload_json = serde_json::json!({ "prompt": prompt }).to_string();
    let job = QueueJob {
        job_id: Uuid::new_v4(),
        run_id: id,
        step: "mockup_generation".to_string(),
        attempt: 1,
        payload_json,
    };
    enqueue(&mut conn, "q.design", &job)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status: RunStatus = row.1.parse().unwrap_or(RunStatus::Draft);
    Ok(Json(CreateRunResponse {
        run: Run {
            id: row.0,
            status,
            created_at: row.2,
        },
    }))
}

async fn select_mockup(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(payload): Json<SelectMockupRequest>,
) -> Result<Json<TransitionRunResponse>, (StatusCode, String)> {
    if payload.mockup_id.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "mockup_id is empty".to_string()));
    }
    let current = get_run_status(&state.db, run_id).await?;
    if current != RunStatus::MockupGenerating && current != RunStatus::MockupReady {
        return Err((
            StatusCode::CONFLICT,
            "run is not ready for mockup selection".to_string(),
        ));
    }
    let run = set_run_status(&state.db, run_id, RunStatus::MockupSelected).await?;
    upsert_step(
        &state.db,
        run_id,
        "mockup_selection".to_string(),
        "completed".to_string(),
        Some(payload.mockup_id.clone()),
    )
    .await?;

    // Fetch the selected mockup data and enqueue Stitch design generation
    let prompt_row = sqlx::query_as::<_, (String,)>("SELECT prompt FROM runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let prompt = prompt_row.map(|r| r.0).unwrap_or_default();

    // Get the banana mockup description for the selected variant
    let mockup_step = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT detail FROM run_steps WHERE run_id = $1 AND step_key = 'mockup_generation'",
    )
    .bind(run_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mockup_detail = mockup_step.and_then(|r| r.0).unwrap_or_default();

    let mut conn = state.redis.get_multiplexed_async_connection().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    enqueue(
        &mut conn,
        "q.stitch",
        &QueueJob {
            job_id: Uuid::new_v4(),
            run_id,
            step: "stitch_generation".to_string(),
            attempt: 1,
            payload_json: serde_json::json!({
                "prompt": prompt,
                "selected_mockup_id": payload.mockup_id,
                "mockup_detail": mockup_detail,
                "source": "mockup_selected"
            })
            .to_string(),
        },
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(TransitionRunResponse { run }))
}

async fn approve_stitch(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(payload): Json<ApproveStitchRequest>,
) -> Result<Json<TransitionRunResponse>, (StatusCode, String)> {
    let current = get_run_status(&state.db, run_id).await?;
    if current != RunStatus::StitchReady {
        return Err((
            StatusCode::CONFLICT,
            format!("run is not at stitch_ready (current: {})", current.as_str()),
        ));
    }
    let run = set_run_status(&state.db, run_id, RunStatus::StitchApproved).await?;
    upsert_step(
        &state.db,
        run_id,
        "stitch_approval".to_string(),
        "completed".to_string(),
        payload.screen_id.or(Some("approved".to_string())),
    )
    .await?;
    Ok(Json(TransitionRunResponse { run }))
}

async fn select_stack(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(payload): Json<SelectStackRequest>,
) -> Result<Json<TransitionRunResponse>, (StatusCode, String)> {
    if payload.stack_id.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "stack_id is empty".to_string()));
    }
    let current = get_run_status(&state.db, run_id).await?;
    if current != RunStatus::StitchApproved && current != RunStatus::MockupSelected {
        return Err((
            StatusCode::CONFLICT,
            format!("run is not at stitch_approved or mockup_selected (current: {})", current.as_str()),
        ));
    }
    let run = set_run_status(&state.db, run_id, RunStatus::StackSelected).await?;
    upsert_step(
        &state.db,
        run_id,
        "stack_selection".to_string(),
        "completed".to_string(),
        Some(payload.stack_id.clone()),
    )
    .await?;

    // Fetch prompt and selected mockup_id, then kick off spec generation
    let prompt_row = sqlx::query_as::<_, (String,)>("SELECT prompt FROM runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mockup_row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT detail FROM run_steps WHERE run_id = $1 AND step_key = 'mockup_selection'",
    )
    .bind(run_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let prompt = prompt_row.map(|r| r.0).unwrap_or_default();
    let selected_mockup_id = mockup_row.and_then(|r| r.0).unwrap_or_else(|| "A".to_string());

    let mut conn = state.redis.get_multiplexed_async_connection().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    enqueue(
        &mut conn,
        "q.spec",
        &QueueJob {
            job_id: Uuid::new_v4(),
            run_id,
            step: "spec_generation".to_string(),
            attempt: 1,
            payload_json: serde_json::json!({
                "prompt": prompt,
                "selected_mockup_id": selected_mockup_id,
                "stack_id": payload.stack_id,
                "source": "user_selection"
            })
            .to_string(),
        },
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(TransitionRunResponse { run }))
}

async fn list_run_steps(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ListRunStepsResponse>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, chrono::DateTime<Utc>)>(
        "SELECT step_key, status, detail, updated_at FROM run_steps WHERE run_id = $1 ORDER BY updated_at DESC",
    )
    .bind(run_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let steps = rows
        .into_iter()
        .map(|row| RunStep {
            step_key: row.0,
            status: row.1,
            detail: row.2,
            updated_at: row.3,
        })
        .collect();
    Ok(Json(ListRunStepsResponse { run_id, steps }))
}

async fn list_run_timeline(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RunTimelineResponse>, (StatusCode, String)> {
    let run = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<Utc>)>(
        "SELECT id, status, created_at FROM runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let Some((_id, status, created_at)) = run else {
        return Err((StatusCode::NOT_FOUND, "run not found".to_string()));
    };
    let mut items = vec![RunTimelineItem {
        at: created_at,
        kind: "run_created".to_string(),
        message: format!("run created with status {}", status),
    }];
    let steps = sqlx::query_as::<_, (String, String, Option<String>, chrono::DateTime<Utc>)>(
        "SELECT step_key, status, detail, updated_at FROM run_steps WHERE run_id = $1 ORDER BY updated_at ASC",
    )
    .bind(run_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    for (step_key, step_status, detail, updated_at) in steps {
        let message = match detail {
            Some(detail) if !detail.trim().is_empty() => {
                format!("{}={} ({})", step_key, step_status, detail)
            }
            _ => format!("{}={}", step_key, step_status),
        };
        items.push(RunTimelineItem {
            at: updated_at,
            kind: "step".to_string(),
            message,
        });
    }
    Ok(Json(RunTimelineResponse { run_id, items }))
}

async fn approve_deploy(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<TransitionRunResponse>, (StatusCode, String)> {
    let current = get_run_status(&state.db, run_id).await?;
    if current != RunStatus::AwaitingApproval && current != RunStatus::PreviewDeployed {
        return Err((
            StatusCode::CONFLICT,
            "run is not awaiting deploy approval".to_string(),
        ));
    }
    let _ = set_run_status(&state.db, run_id, RunStatus::ProdDeploying).await?;
    upsert_step(
        &state.db,
        run_id,
        "deploy_approval".to_string(),
        "approved".to_string(),
        Some("manual approval received".to_string()),
    )
    .await?;
    let run = set_run_status(&state.db, run_id, RunStatus::Done).await?;
    upsert_step(
        &state.db,
        run_id,
        "production_deploy".to_string(),
        "completed".to_string(),
        Some(format!("completed_at={}", Utc::now().to_rfc3339())),
    )
    .await?;
    Ok(Json(TransitionRunResponse { run }))
}

async fn reject_deploy(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(payload): Json<RejectDeployRequest>,
) -> Result<Json<TransitionRunResponse>, (StatusCode, String)> {
    let current = get_run_status(&state.db, run_id).await?;
    if current != RunStatus::AwaitingApproval && current != RunStatus::PreviewDeployed {
        return Err((
            StatusCode::CONFLICT,
            "run is not awaiting deploy approval".to_string(),
        ));
    }
    let run = set_run_status(&state.db, run_id, RunStatus::Cancelled).await?;
    upsert_step(
        &state.db,
        run_id,
        "deploy_approval".to_string(),
        "rejected".to_string(),
        payload.reason,
    )
    .await?;
    Ok(Json(TransitionRunResponse { run }))
}

async fn metrics_summary(
    State(state): State<AppState>,
) -> Result<Json<MetricsSummaryResponse>, (StatusCode, String)> {
    let total_runs = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM runs")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let running_runs = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM runs WHERE status IN ('mockup_generating','spec_generating','codegen_running','ci_running','prod_deploying')",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let failed_runs = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM runs WHERE status IN ('failed_retryable','failed_final')",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let done_runs = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM runs WHERE status = 'done'")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let audit_logs = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_logs")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(MetricsSummaryResponse {
        total_runs,
        running_runs,
        failed_runs,
        done_runs,
        audit_logs,
    }))
}

async fn list_connections(
    State(state): State<AppState>,
) -> Result<Json<ListConnectionsResponse>, (StatusCode, String)> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            serde_json::Value,
            chrono::DateTime<Utc>,
            Option<chrono::DateTime<Utc>>,
        ),
    >(
        "SELECT provider, scopes, updated_at, revoked_at FROM connections ORDER BY provider ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut connections = Vec::new();
    for (provider, scopes_value, updated_at, revoked_at) in rows {
        let provider = ConnectionProvider::from_str(&provider).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid provider".to_string(),
            )
        })?;
        let scopes: Vec<String> = serde_json::from_value(scopes_value)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        connections.push(Connection {
            provider,
            scopes,
            connected: revoked_at.is_none(),
            updated_at,
        });
    }
    Ok(Json(ListConnectionsResponse { connections }))
}

async fn upsert_connection(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Json(payload): Json<UpsertConnectionRequest>,
) -> Result<Json<UpsertConnectionResponse>, (StatusCode, String)> {
    if payload.access_token.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "access_token is empty".to_string()));
    }
    let provider = ConnectionProvider::from_str(&provider)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid provider".to_string()))?;
    let access_token_detail = masked_token_detail(&payload.access_token);
    let refresh_token_detail = payload
        .refresh_token
        .as_deref()
        .filter(|token| !token.trim().is_empty())
        .map(masked_token_detail);
    let external_account_id_present = payload.external_account_id.is_some();
    let encrypted_access_token = encrypt_token(&state.encryption_key, &payload.access_token)?;
    let encrypted_refresh_token = match payload.refresh_token.as_deref() {
        Some(token) if !token.trim().is_empty() => {
            Some(encrypt_token(&state.encryption_key, token)?)
        }
        _ => None,
    };
    let scopes = serde_json::to_value(&payload.scopes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let row = sqlx::query_as::<_, (String, serde_json::Value, chrono::DateTime<Utc>)>(
        "INSERT INTO connections (provider, encrypted_access_token, encrypted_refresh_token, external_account_id, scopes)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider) DO UPDATE SET
           encrypted_access_token = EXCLUDED.encrypted_access_token,
           encrypted_refresh_token = EXCLUDED.encrypted_refresh_token,
           external_account_id = EXCLUDED.external_account_id,
           scopes = EXCLUDED.scopes,
           revoked_at = NULL,
           updated_at = now()
         RETURNING provider, scopes, updated_at",
    )
    .bind(provider.as_str())
    .bind(encrypted_access_token)
    .bind(encrypted_refresh_token)
    .bind(payload.external_account_id)
    .bind(scopes)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let scopes: Vec<String> = serde_json::from_value(row.1)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = write_audit(
        &state.db,
        Some(provider),
        "connection_upsert",
        "success",
        serde_json::json!({
            "scopes_count": scopes.len(),
            "access_token": access_token_detail,
            "refresh_token": refresh_token_detail,
            "external_account_id_present": external_account_id_present
        }),
    )
    .await;
    Ok(Json(UpsertConnectionResponse {
        connection: Connection {
            provider,
            scopes,
            connected: true,
            updated_at: row.2,
        },
    }))
}

async fn delete_connection(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<Json<DeleteConnectionResponse>, (StatusCode, String)> {
    let provider = ConnectionProvider::from_str(&provider)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid provider".to_string()))?;
    let deleted = sqlx::query("DELETE FROM connections WHERE provider = $1")
        .bind(provider.as_str())
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .rows_affected()
        > 0;
    let _ = write_audit(
        &state.db,
        Some(provider),
        "connection_delete",
        "success",
        serde_json::json!({ "deleted": deleted }),
    )
    .await;
    Ok(Json(DeleteConnectionResponse { provider, deleted }))
}

async fn refresh_connection_token(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<Json<RefreshConnectionResponse>, (StatusCode, String)> {
    let provider = ConnectionProvider::from_str(&provider)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid provider".to_string()))?;
    let config = oauth_provider_config(provider)?;
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT encrypted_access_token, encrypted_refresh_token
         FROM connections WHERE provider = $1 AND revoked_at IS NULL",
    )
    .bind(provider.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let Some((_encrypted_access_token, encrypted_refresh_token)) = row else {
        return Err((StatusCode::NOT_FOUND, "connection not found".to_string()));
    };
    let Some(refresh_token_cipher) = encrypted_refresh_token else {
        return Err((
            StatusCode::BAD_REQUEST,
            "refresh token is missing".to_string(),
        ));
    };
    let refresh_token = decrypt_token(&state.encryption_key, &refresh_token_cipher)?;
    let refresh_token_before_detail = masked_token_detail(&refresh_token);

    let token_res = state
        .http
        .post(&config.token_url)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
        ])
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    if !token_res.status().is_success() {
        let _ = write_audit(
            &state.db,
            Some(provider),
            "connection_refresh",
            "failed",
            serde_json::json!({
                "reason": "token_exchange_failed",
                "upstream_status": token_res.status().as_u16()
            }),
        )
        .await;
        return Err((
            StatusCode::BAD_GATEWAY,
            "refresh token exchange failed".to_string(),
        ));
    }
    let token_body: OAuthTokenResponse = token_res
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    if token_body.access_token.trim().is_empty() {
        return Err((StatusCode::BAD_GATEWAY, "missing access token".to_string()));
    }

    let scopes = token_body
        .scope
        .map(|s| {
            s.split_whitespace()
                .filter(|x| !x.trim().is_empty())
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or(config.scopes);
    let encrypted_access_token = encrypt_token(&state.encryption_key, &token_body.access_token)?;
    let next_refresh_token = token_body
        .refresh_token
        .as_deref()
        .unwrap_or(refresh_token.as_str());
    let access_token_after_detail = masked_token_detail(&token_body.access_token);
    let refresh_token_after_detail = masked_token_detail(next_refresh_token);
    let encrypted_refresh_token = encrypt_token(&state.encryption_key, next_refresh_token)?;
    let scopes_json = serde_json::to_value(&scopes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let expires_at = token_body
        .expires_in
        .map(|s| Utc::now() + chrono::Duration::seconds(i64::from(s)));

    let row = sqlx::query_as::<_, (serde_json::Value, chrono::DateTime<Utc>)>(
        "UPDATE connections
         SET encrypted_access_token = $2,
             encrypted_refresh_token = $3,
             scopes = $4,
             token_expires_at = $5,
             last_rotated_at = now(),
             revoked_at = NULL,
             updated_at = now()
         WHERE provider = $1
         RETURNING scopes, updated_at",
    )
    .bind(provider.as_str())
    .bind(encrypted_access_token)
    .bind(encrypted_refresh_token)
    .bind(scopes_json)
    .bind(expires_at)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let Some((scopes_value, updated_at)) = row else {
        return Err((StatusCode::NOT_FOUND, "connection not found".to_string()));
    };
    let scopes: Vec<String> = serde_json::from_value(scopes_value)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = write_audit(
        &state.db,
        Some(provider),
        "connection_refresh",
        "success",
        serde_json::json!({
            "scopes_count": scopes.len(),
            "access_token_after": access_token_after_detail,
            "refresh_token_before": refresh_token_before_detail,
            "refresh_token_after": refresh_token_after_detail
        }),
    )
    .await;
    Ok(Json(RefreshConnectionResponse {
        connection: Connection {
            provider,
            scopes,
            connected: true,
            updated_at,
        },
    }))
}

async fn revoke_connection(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<Json<RevokeConnectionResponse>, (StatusCode, String)> {
    let provider = ConnectionProvider::from_str(&provider)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid provider".to_string()))?;
    let config = oauth_provider_config(provider)?;
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT encrypted_access_token, encrypted_refresh_token FROM connections WHERE provider = $1",
    )
    .bind(provider.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let Some((access_cipher, refresh_cipher)) = row else {
        let _ = write_audit(
            &state.db,
            Some(provider),
            "connection_revoke",
            "success",
            serde_json::json!({ "revoked": false, "reason": "connection_not_found" }),
        )
        .await;
        return Ok(Json(RevokeConnectionResponse {
            provider,
            revoked: false,
        }));
    };

    let mut remote_revoke_attempted = false;
    let mut remote_revoke_ok = None;
    let mut token_used_detail = None;
    if let Some(revoke_url) = config.revoke_url {
        let token = if let Some(cipher) = access_cipher {
            decrypt_token(&state.encryption_key, &cipher)?
        } else if let Some(cipher) = refresh_cipher {
            decrypt_token(&state.encryption_key, &cipher)?
        } else {
            String::new()
        };
        if !token.is_empty() {
            remote_revoke_attempted = true;
            token_used_detail = Some(masked_token_detail(&token));
            remote_revoke_ok = Some(
                state
                .http
                .post(revoke_url)
                .form(&[
                    ("token", token.as_str()),
                    ("client_id", config.client_id.as_str()),
                    ("client_secret", config.client_secret.as_str()),
                ])
                .send()
                .await
                .map(|resp| resp.status().is_success())
                .unwrap_or(false),
            );
        }
    }

    let revoked = sqlx::query(
        "UPDATE connections
         SET encrypted_access_token = NULL,
             encrypted_refresh_token = NULL,
             revoked_at = now(),
             updated_at = now()
         WHERE provider = $1",
    )
    .bind(provider.as_str())
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .rows_affected()
        > 0;
    let _ = write_audit(
        &state.db,
        Some(provider),
        "connection_revoke",
        "success",
        serde_json::json!({
            "revoked": revoked,
            "remote_revoke_attempted": remote_revoke_attempted,
            "remote_revoke_ok": remote_revoke_ok,
            "token_used": token_used_detail
        }),
    )
    .await;
    Ok(Json(RevokeConnectionResponse { provider, revoked }))
}

async fn oauth_start_connection(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Json(payload): Json<OAuthStartRequest>,
) -> Result<Json<OAuthStartResponse>, (StatusCode, String)> {
    let provider = ConnectionProvider::from_str(&provider)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid provider".to_string()))?;
    let config = oauth_provider_config(provider)?;
    let redirect_uri = payload.redirect_uri.unwrap_or(config.redirect_uri);
    let state_value = random_urlsafe(24);
    let verifier = random_urlsafe(48);
    let challenge = pkce_challenge(&verifier);
    let expires_at = Utc::now() + chrono::Duration::minutes(10);

    sqlx::query(
        "INSERT INTO oauth_states (state, provider, redirect_uri, code_verifier, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&state_value)
    .bind(provider.as_str())
    .bind(&redirect_uri)
    .bind(&verifier)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = write_audit(
        &state.db,
        Some(provider),
        "oauth_start",
        "success",
        serde_json::json!({
            "state_hint": state_hint(&state_value),
            "redirect_uri": redirect_uri.clone(),
            "scopes_count": config.scopes.len()
        }),
    )
    .await;

    let mut url = Url::parse(&config.auth_url)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("response_type", "code");
        qp.append_pair("client_id", &config.client_id);
        qp.append_pair("redirect_uri", &redirect_uri);
        qp.append_pair("scope", &config.scopes.join(" "));
        qp.append_pair("state", &state_value);
        qp.append_pair("code_challenge", &challenge);
        qp.append_pair("code_challenge_method", "S256");
    }

    Ok(Json(OAuthStartResponse {
        authorize_url: url.to_string(),
        state: state_value,
    }))
}

async fn oauth_callback_connection(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Json(payload): Json<OAuthCallbackRequest>,
) -> Result<Json<UpsertConnectionResponse>, (StatusCode, String)> {
    let provider = ConnectionProvider::from_str(&provider)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid provider".to_string()))?;
    let config = oauth_provider_config(provider)?;
    let row = sqlx::query_as::<_, (String, String, String, DateTime<Utc>)>(
        "SELECT redirect_uri, code_verifier, provider, expires_at
         FROM oauth_states WHERE state = $1",
    )
    .bind(&payload.state)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let Some((redirect_uri, code_verifier, saved_provider, expires_at)) = row else {
        let _ = write_audit(
            &state.db,
            Some(provider),
            "oauth_callback",
            "failed",
            serde_json::json!({
                "reason": "invalid_state",
                "state_hint": state_hint(&payload.state)
            }),
        )
        .await;
        return Err((StatusCode::BAD_REQUEST, "invalid state".to_string()));
    };
    if saved_provider != provider.as_str() {
        let _ = write_audit(
            &state.db,
            Some(provider),
            "oauth_callback",
            "failed",
            serde_json::json!({
                "reason": "state_provider_mismatch",
                "state_hint": state_hint(&payload.state)
            }),
        )
        .await;
        return Err((
            StatusCode::BAD_REQUEST,
            "state/provider mismatch".to_string(),
        ));
    }
    if expires_at < Utc::now() {
        let _ = write_audit(
            &state.db,
            Some(provider),
            "oauth_callback",
            "failed",
            serde_json::json!({
                "reason": "state_expired",
                "state_hint": state_hint(&payload.state)
            }),
        )
        .await;
        return Err((StatusCode::BAD_REQUEST, "state expired".to_string()));
    }

    sqlx::query("DELETE FROM oauth_states WHERE state = $1")
        .bind(&payload.state)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let token_res = state
        .http
        .post(&config.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", payload.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code_verifier", code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    if !token_res.status().is_success() {
        let _ = write_audit(
            &state.db,
            Some(provider),
            "oauth_callback",
            "failed",
            serde_json::json!({
                "reason": "token_exchange_failed",
                "upstream_status": token_res.status().as_u16()
            }),
        )
        .await;
        return Err((
            StatusCode::BAD_GATEWAY,
            "oauth token exchange failed".to_string(),
        ));
    }
    let token_body: OAuthTokenResponse = token_res
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    if token_body.access_token.trim().is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "oauth response missing access_token".to_string(),
        ));
    }

    let scopes = token_body
        .scope
        .map(|s| {
            s.split_whitespace()
                .filter(|x| !x.trim().is_empty())
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or(config.scopes);

    let encrypted_access_token = encrypt_token(&state.encryption_key, &token_body.access_token)?;
    let encrypted_refresh_token = match token_body.refresh_token.as_deref() {
        Some(token) if !token.trim().is_empty() => {
            Some(encrypt_token(&state.encryption_key, token)?)
        }
        _ => None,
    };
    let expires_at = token_body
        .expires_in
        .map(|s| Utc::now() + chrono::Duration::seconds(i64::from(s)));
    let scopes_json = serde_json::to_value(&scopes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let row = sqlx::query_as::<_, (String, serde_json::Value, DateTime<Utc>)>(
        "INSERT INTO connections (provider, encrypted_access_token, encrypted_refresh_token, external_account_id, scopes)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider) DO UPDATE SET
           encrypted_access_token = EXCLUDED.encrypted_access_token,
           encrypted_refresh_token = EXCLUDED.encrypted_refresh_token,
           external_account_id = EXCLUDED.external_account_id,
           scopes = EXCLUDED.scopes,
           token_expires_at = $6,
           last_rotated_at = now(),
           revoked_at = NULL,
           updated_at = now()
         RETURNING provider, scopes, updated_at",
    )
    .bind(provider.as_str())
    .bind(encrypted_access_token)
    .bind(encrypted_refresh_token)
    .bind(token_body.external_account_id)
    .bind(scopes_json)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let scopes: Vec<String> = serde_json::from_value(row.1)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = write_audit(
        &state.db,
        Some(provider),
        "oauth_callback",
        "success",
        serde_json::json!({
            "state_hint": state_hint(&payload.state),
            "scopes_count": scopes.len(),
            "access_token": masked_token_detail(&token_body.access_token),
            "refresh_token": token_body.refresh_token.as_deref().filter(|token| !token.trim().is_empty()).map(masked_token_detail)
        }),
    )
    .await;
    Ok(Json(UpsertConnectionResponse {
        connection: Connection {
            provider,
            scopes,
            connected: true,
            updated_at: row.2,
        },
    }))
}

async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Run>, (StatusCode, String)> {
    let row = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<Utc>)>(
        "SELECT id, status, created_at FROM runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let Some((id, status_json, created_at)) = row else {
        return Err((StatusCode::NOT_FOUND, "run not found".to_string()));
    };
    let status: RunStatus = status_json.parse().unwrap_or(RunStatus::Draft);
    Ok(Json(Run {
        id,
        status,
        created_at,
    }))
}

async fn get_run_status(db: &PgPool, run_id: Uuid) -> Result<RunStatus, (StatusCode, String)> {
    let status = sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let Some(status) = status else {
        return Err((StatusCode::NOT_FOUND, "run not found".to_string()));
    };
    Ok(status.parse().unwrap_or(RunStatus::Draft))
}

async fn set_run_status(
    db: &PgPool,
    run_id: Uuid,
    status: RunStatus,
) -> Result<Run, (StatusCode, String)> {
    let row = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<Utc>)>(
        "UPDATE runs SET status = $2 WHERE id = $1 RETURNING id, status, created_at",
    )
    .bind(run_id)
    .bind(status.as_str())
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let Some((id, db_status, created_at)) = row else {
        return Err((StatusCode::NOT_FOUND, "run not found".to_string()));
    };
    Ok(Run {
        id,
        status: db_status.parse().unwrap_or(status),
        created_at,
    })
}

async fn upsert_step(
    db: &PgPool,
    run_id: Uuid,
    step_key: String,
    status: String,
    detail: Option<String>,
) -> Result<(), (StatusCode, String)> {
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
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

async fn write_audit(
    db: &PgPool,
    provider: Option<ConnectionProvider>,
    action: &str,
    status: &str,
    detail: serde_json::Value,
) -> Result<(), (StatusCode, String)> {
    sqlx::query("INSERT INTO audit_logs (provider, action, status, detail) VALUES ($1, $2, $3, $4)")
        .bind(provider.map(|p| p.as_str().to_string()))
        .bind(action)
        .bind(status)
        .bind(detail)
        .execute(db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

async fn run_events_sse(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let heartbeat = state.sse_heartbeat;

    let stream = stream::unfold(
        (state, run_id, None::<RunStatus>, None::<DateTime<Utc>>),
        move |(state, run_id, last_status, last_step_at)| async move {
        time::sleep(heartbeat).await;
        let mut next_last_status = last_status;
        let mut next_last_step_at = last_step_at;
        let mut evt = SseEvent::Heartbeat { at: Utc::now() };
        let mut event_name = "heartbeat".to_string();

        let run_status = sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .and_then(|status| status.parse::<RunStatus>().ok());

        if let Some(status) = run_status
            && next_last_status != Some(status)
        {
            evt = match status {
                RunStatus::Done => {
                    event_name = "run_completed".to_string();
                    SseEvent::RunCompleted { at: Utc::now() }
                }
                RunStatus::FailedFinal | RunStatus::Cancelled => {
                    event_name = "run_failed".to_string();
                    SseEvent::RunFailed {
                        at: Utc::now(),
                        reason: status.as_str().to_string(),
                    }
                }
                _ => {
                    event_name = "state_changed".to_string();
                    SseEvent::StateChanged {
                        at: Utc::now(),
                        status,
                    }
                }
            };
            next_last_status = Some(status);
        }

        if event_name == "heartbeat" {
            let next_step = sqlx::query_as::<_, (String, String, Option<String>, DateTime<Utc>)>(
                "SELECT step_key, status, detail, updated_at
                 FROM run_steps
                 WHERE run_id = $1
                   AND ($2::timestamptz IS NULL OR updated_at > $2)
                 ORDER BY updated_at ASC
                 LIMIT 1",
            )
            .bind(run_id)
            .bind(next_last_step_at)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
            if let Some((step_key, step_status, detail, updated_at)) = next_step {
                let message = match detail {
                    Some(detail) if !detail.trim().is_empty() => {
                        format!("{}={} ({})", step_key, step_status, detail)
                    }
                    _ => format!("{}={}", step_key, step_status),
                };
                evt = SseEvent::StepLog {
                    at: updated_at,
                    message,
                };
                event_name = "step_log".to_string();
                next_last_step_at = Some(updated_at);
            }
        }

        let data = serde_json::to_string(&evt).unwrap_or_else(|_| "{}".to_string());
        Some((
            Ok(Event::default().event(&event_name).data(data)),
            (state, run_id, next_last_status, next_last_step_at),
        ))
    },
    );

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

#[derive(Debug, Clone)]
struct OAuthProviderConfig {
    client_id: String,
    client_secret: String,
    auth_url: String,
    token_url: String,
    revoke_url: Option<String>,
    redirect_uri: String,
    scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    scope: Option<String>,
    external_account_id: Option<String>,
    expires_in: Option<u32>,
}

fn read_encryption_key() -> anyhow::Result<[u8; 32]> {
    let raw = std::env::var("ENCRYPTION_KEY")
        .unwrap_or_else(|_| "0123456789abcdef0123456789abcdef".to_string());
    if raw.len() == 32 {
        let mut key = [0u8; 32];
        key.copy_from_slice(raw.as_bytes());
        return Ok(key);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if decoded.len() != 32 {
        return Err(anyhow::anyhow!("ENCRYPTION_KEY must be 32 bytes"));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}

fn encrypt_token(key: &[u8; 32], plaintext: &str) -> Result<String, (StatusCode, String)> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(combined))
}

fn decrypt_token(key: &[u8; 32], encoded: &str) -> Result<String, (StatusCode, String)> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if bytes.len() < 13 {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid cipher payload".to_string(),
        ));
    }
    let nonce = Nonce::from_slice(&bytes[..12]);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let plaintext = cipher
        .decrypt(nonce, &bytes[12..])
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    String::from_utf8(plaintext).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

fn masked_token_detail(token: &str) -> serde_json::Value {
    serde_json::json!({
        "masked": masked_token(token),
        "sha256_prefix": sha256_prefix(token),
        "length": token.len()
    })
}

fn masked_token(token: &str) -> String {
    if token.len() <= 8 {
        return "****".to_string();
    }
    format!("{}***{}", &token[..4], &token[token.len() - 4..])
}

fn sha256_prefix(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest[..6]
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>()
}

fn state_hint(value: &str) -> String {
    if value.len() <= 8 {
        return "****".to_string();
    }
    format!("{}***{}", &value[..4], &value[value.len() - 4..])
}

fn random_urlsafe(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn oauth_provider_config(
    provider: ConnectionProvider,
) -> Result<OAuthProviderConfig, (StatusCode, String)> {
    let key = provider.as_str().to_ascii_uppercase();
    let client_id = read_env_required(&format!("OAUTH_{}_CLIENT_ID", key))?;
    let client_secret = read_env_required(&format!("OAUTH_{}_CLIENT_SECRET", key))?;
    let auth_url = read_env_required(&format!("OAUTH_{}_AUTH_URL", key))?;
    let token_url = read_env_required(&format!("OAUTH_{}_TOKEN_URL", key))?;
    let revoke_url = std::env::var(format!("OAUTH_{}_REVOKE_URL", key)).ok();
    let redirect_uri = read_env_required(&format!("OAUTH_{}_REDIRECT_URL", key))?;
    let scopes_value = read_env_required(&format!("OAUTH_{}_SCOPES", key))?;
    let scopes = scopes_value
        .split([',', ' '])
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>();
    if scopes.is_empty() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "oauth scopes empty".to_string(),
        ));
    }
    Ok(OAuthProviderConfig {
        client_id,
        client_secret,
        auth_url,
        token_url,
        revoke_url,
        redirect_uri,
        scopes,
    })
}

fn read_env_required(key: &str) -> Result<String, (StatusCode, String)> {
    std::env::var(key).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("missing {}", key),
        )
    })
}
