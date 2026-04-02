use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type RunId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Draft,
    MockupGenerating,
    MockupReady,
    MockupSelected,
    StackSelected,
    ContractLocked,
    SpecGenerating,
    CodegenRunning,
    CiRunning,
    PrReady,
    PreviewDeployed,
    AwaitingApproval,
    ProdDeploying,
    Done,
    FailedRetryable,
    FailedFinal,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Draft => "draft",
            RunStatus::MockupGenerating => "mockup_generating",
            RunStatus::MockupReady => "mockup_ready",
            RunStatus::MockupSelected => "mockup_selected",
            RunStatus::StackSelected => "stack_selected",
            RunStatus::ContractLocked => "contract_locked",
            RunStatus::SpecGenerating => "spec_generating",
            RunStatus::CodegenRunning => "codegen_running",
            RunStatus::CiRunning => "ci_running",
            RunStatus::PrReady => "pr_ready",
            RunStatus::PreviewDeployed => "preview_deployed",
            RunStatus::AwaitingApproval => "awaiting_approval",
            RunStatus::ProdDeploying => "prod_deploying",
            RunStatus::Done => "done",
            RunStatus::FailedRetryable => "failed_retryable",
            RunStatus::FailedFinal => "failed_final",
            RunStatus::Cancelled => "cancelled",
        }
    }
}

impl std::str::FromStr for RunStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(RunStatus::Draft),
            "mockup_generating" => Ok(RunStatus::MockupGenerating),
            "mockup_ready" => Ok(RunStatus::MockupReady),
            "mockup_selected" => Ok(RunStatus::MockupSelected),
            "stack_selected" => Ok(RunStatus::StackSelected),
            "contract_locked" => Ok(RunStatus::ContractLocked),
            "spec_generating" => Ok(RunStatus::SpecGenerating),
            "codegen_running" => Ok(RunStatus::CodegenRunning),
            "ci_running" => Ok(RunStatus::CiRunning),
            "pr_ready" => Ok(RunStatus::PrReady),
            "preview_deployed" => Ok(RunStatus::PreviewDeployed),
            "awaiting_approval" => Ok(RunStatus::AwaitingApproval),
            "prod_deploying" => Ok(RunStatus::ProdDeploying),
            "done" => Ok(RunStatus::Done),
            "failed_retryable" => Ok(RunStatus::FailedRetryable),
            "failed_final" => Ok(RunStatus::FailedFinal),
            "cancelled" => Ok(RunStatus::Cancelled),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRunRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRunResponse {
    pub run: Run,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectMockupRequest {
    pub mockup_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectStackRequest {
    pub stack_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionRunResponse {
    pub run: Run,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectDeployRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTimelineItem {
    pub at: DateTime<Utc>,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTimelineResponse {
    pub run_id: RunId,
    pub items: Vec<RunTimelineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummaryResponse {
    pub total_runs: i64,
    pub running_runs: i64,
    pub failed_runs: i64,
    pub done_runs: i64,
    pub audit_logs: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionProvider {
    Banana,
    Stitch,
    Claude,
    Github,
    Vercel,
}

impl ConnectionProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionProvider::Banana => "banana",
            ConnectionProvider::Stitch => "stitch",
            ConnectionProvider::Claude => "claude",
            ConnectionProvider::Github => "github",
            ConnectionProvider::Vercel => "vercel",
        }
    }
}

impl std::str::FromStr for ConnectionProvider {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "banana" => Ok(ConnectionProvider::Banana),
            "stitch" => Ok(ConnectionProvider::Stitch),
            "claude" => Ok(ConnectionProvider::Claude),
            "github" => Ok(ConnectionProvider::Github),
            "vercel" => Ok(ConnectionProvider::Vercel),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertConnectionRequest {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub external_account_id: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthStartRequest {
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthStartResponse {
    pub authorize_url: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCallbackRequest {
    pub state: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub provider: ConnectionProvider,
    pub scopes: Vec<String>,
    pub connected: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListConnectionsResponse {
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertConnectionResponse {
    pub connection: Connection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteConnectionResponse {
    pub provider: ConnectionProvider,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshConnectionResponse {
    pub connection: Connection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeConnectionResponse {
    pub provider: ConnectionProvider,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStep {
    pub step_key: String,
    pub status: String,
    pub detail: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRunStepsResponse {
    pub run_id: RunId,
    pub steps: Vec<RunStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseEvent {
    Heartbeat {
        at: DateTime<Utc>,
    },
    StateChanged {
        at: DateTime<Utc>,
        status: RunStatus,
    },
    StepLog {
        at: DateTime<Utc>,
        message: String,
    },
    ArtifactReady {
        at: DateTime<Utc>,
        artifact_key: String,
    },
    GateResult {
        at: DateTime<Utc>,
        gate: String,
        passed: bool,
    },
    RunFailed {
        at: DateTime<Utc>,
        reason: String,
    },
    RunCompleted {
        at: DateTime<Utc>,
    },
}
