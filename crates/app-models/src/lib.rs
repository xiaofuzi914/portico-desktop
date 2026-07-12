//! Core product types shared across the Portico desktop agent.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Unique identifier for a user workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceId(pub uuid::Uuid);

impl WorkspaceId {
    /// Create a new random workspace identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a conversation thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadId(pub uuid::Uuid);

impl ThreadId {
    /// Create a new random thread identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for an agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunId(pub uuid::Uuid);

impl AgentRunId {
    /// Create a new random agent run identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for AgentRunId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a persisted conversation message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MessageId(pub uuid::Uuid);

impl MessageId {
    /// Create a new random message identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

/// High-level lifecycle status of an agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum AgentRunStatus {
    /// Run is queued but has not started yet.
    Queued,
    /// Run is actively executing.
    Running,
    /// Run is waiting for user approval.
    WaitingApproval,
    /// Run is paused.
    Paused,
    /// Run was cancelled.
    Cancelled,
    /// Run failed.
    Failed,
    /// The application stopped while the run was active.
    Interrupted,
    /// Run completed successfully.
    Completed,
}

impl AgentRunStatus {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::WaitingApproval => "WaitingApproval",
            Self::Paused => "Paused",
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
            Self::Interrupted => "Interrupted",
            Self::Completed => "Completed",
        }
    }

    /// Whether the status represents a finished run.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Cancelled | Self::Failed | Self::Interrupted | Self::Completed
        )
    }
}

impl TryFrom<&str> for AgentRunStatus {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Queued" => Ok(Self::Queued),
            "Running" => Ok(Self::Running),
            "WaitingApproval" => Ok(Self::WaitingApproval),
            "Paused" => Ok(Self::Paused),
            "Cancelled" => Ok(Self::Cancelled),
            "Failed" => Ok(Self::Failed),
            "Interrupted" => Ok(Self::Interrupted),
            "Completed" => Ok(Self::Completed),
            _ => Err(AppError::Internal {
                message: format!("unknown run status: {value}"),
            }),
        }
    }
}

/// A user workspace.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Workspace {
    /// Workspace identifier.
    pub id: WorkspaceId,
    /// Display name.
    pub name: String,
    /// Root directory path.
    pub root_path: String,
    /// Whether the workspace is trusted for sensitive operations.
    pub trusted: bool,
    /// Paths explicitly allowed for read access.
    pub allowed_read_paths: Vec<String>,
    /// Paths explicitly allowed for write access.
    pub allowed_write_paths: Vec<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Unique identifier for a git/terminal worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorktreeId(pub uuid::Uuid);

impl WorktreeId {
    /// Create a new random worktree identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for WorktreeId {
    fn default() -> Self {
        Self::new()
    }
}

/// A checked-out worktree linked to a workspace and thread.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Worktree {
    /// Worktree identifier.
    pub id: WorktreeId,
    /// Owning workspace.
    pub workspace_id: WorkspaceId,
    /// Owning thread.
    pub thread_id: ThreadId,
    /// Display name.
    pub name: String,
    /// Filesystem path.
    pub path: String,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Unique identifier for a terminal session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TerminalId(pub uuid::Uuid);

impl TerminalId {
    /// Create a new random terminal identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for TerminalId {
    fn default() -> Self {
        Self::new()
    }
}

/// Output of a terminal command execution.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TerminalOutput {
    /// Standard output captured from the command.
    pub stdout: String,
    /// Standard error captured from the command.
    pub stderr: String,
    /// Process exit code, if the process exited.
    pub exit_code: Option<i32>,
}

/// A conversation thread inside a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Thread {
    /// Thread identifier.
    pub id: ThreadId,
    /// Owning workspace.
    pub workspace_id: WorkspaceId,
    /// Display title.
    pub title: String,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// The author role of a durable conversation message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum MessageRole {
    /// A human-authored message.
    User,
    /// A response from the configured model.
    Assistant,
    /// A system-generated message.
    System,
}

impl MessageRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Assistant => "Assistant",
            Self::System => "System",
        }
    }
}

impl TryFrom<&str> for MessageRole {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "User" => Ok(Self::User),
            "Assistant" => Ok(Self::Assistant),
            "System" => Ok(Self::System),
            _ => Err(AppError::Internal {
                message: format!("unknown message role: {value}"),
            }),
        }
    }
}

/// A durable entry in a thread conversation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Message {
    pub id: MessageId,
    pub thread_id: ThreadId,
    pub run_id: Option<AgentRunId>,
    pub role: MessageRole,
    pub content: String,
    pub client_request_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A single agent run inside a workspace and thread.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRun {
    /// Run identifier.
    pub id: AgentRunId,
    /// Thread the run belongs to.
    pub thread_id: ThreadId,
    /// Workspace the run belongs to.
    pub workspace_id: WorkspaceId,
    /// Current lifecycle status.
    pub status: AgentRunStatus,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp when the run started running.
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Timestamp when the run finished.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Authoritative product context attached to every capability execution.
///
/// Callers must obtain this from the runtime using a persisted run id. Trust,
/// ownership, roots, and allowed paths are snapshots of backend-owned state and
/// must never be accepted from a frontend or model payload.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionContext {
    /// Run requesting the capability.
    pub run_id: AgentRunId,
    /// Thread that owns the run.
    pub thread_id: ThreadId,
    /// Workspace that owns the thread and run.
    pub workspace_id: WorkspaceId,
    /// Canonical workspace root at preparation time.
    pub canonical_workspace_root: String,
    /// Persisted trust state at preparation time.
    pub trusted_workspace: bool,
    /// Canonical paths explicitly granted for reads.
    pub allowed_read_paths: Vec<String>,
    /// Canonical paths explicitly granted for writes.
    pub allowed_write_paths: Vec<String>,
    /// Workspace update timestamp used as the trust/config revision.
    pub trust_revision: chrono::DateTime<chrono::Utc>,
    /// Correlation id shared by policy, approval, execution, and audit records.
    pub correlation_id: uuid::Uuid,
}

/// Stable identifier for a durable tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolInvocationId(pub uuid::Uuid);

impl ToolInvocationId {
    /// Create a new random invocation identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for ToolInvocationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Durable lifecycle for one prepared tool side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ToolInvocationStatus {
    /// Policy allowed the invocation and it may be claimed by a worker.
    Ready,
    /// The invocation is blocked on a human decision.
    WaitingApproval,
    /// Approval was granted and the invocation may be claimed.
    Approved,
    /// A worker owns the invocation lease and may execute it.
    Executing,
    /// The tool produced a durable successful result.
    Succeeded,
    /// The tool failed and will not be automatically replayed.
    Failed,
    /// Policy or a human denied the invocation before execution.
    Denied,
    /// The owning run was cancelled before execution.
    Cancelled,
    /// Execution ownership was lost during a process restart; reconciliation is required.
    NeedsReconciliation,
}

impl ToolInvocationStatus {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::WaitingApproval => "WaitingApproval",
            Self::Approved => "Approved",
            Self::Executing => "Executing",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Denied => "Denied",
            Self::Cancelled => "Cancelled",
            Self::NeedsReconciliation => "NeedsReconciliation",
        }
    }

    /// Whether no further automatic execution is permitted.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Denied | Self::Cancelled
        )
    }
}

impl TryFrom<&str> for ToolInvocationStatus {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Ready" => Ok(Self::Ready),
            "WaitingApproval" => Ok(Self::WaitingApproval),
            "Approved" => Ok(Self::Approved),
            "Executing" => Ok(Self::Executing),
            "Succeeded" => Ok(Self::Succeeded),
            "Failed" => Ok(Self::Failed),
            "Denied" => Ok(Self::Denied),
            "Cancelled" => Ok(Self::Cancelled),
            "NeedsReconciliation" => Ok(Self::NeedsReconciliation),
            _ => Err(AppError::Internal {
                message: format!("unknown tool invocation status: {value}"),
            }),
        }
    }
}

/// Immutable prepared request plus its durable execution receipt.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolInvocation {
    /// Stable invocation identifier.
    pub id: ToolInvocationId,
    /// Owning run.
    pub run_id: AgentRunId,
    /// Owning thread.
    pub thread_id: ThreadId,
    /// Owning workspace.
    pub workspace_id: WorkspaceId,
    /// Provider/model tool-call id. Unique within a run when supplied.
    pub model_call_id: Option<String>,
    /// Product tool name.
    pub tool_name: String,
    /// Tool implementation/version snapshot.
    pub tool_version: String,
    /// Policy action, such as `filesystem.read` or `filesystem.write`.
    pub action: String,
    /// Canonical, human-readable resource summary.
    pub resource: String,
    /// Immutable validated arguments.
    #[ts(type = "any")]
    pub arguments: serde_json::Value,
    /// SHA-256 fingerprint over context, tool metadata, and canonical arguments.
    pub request_hash: String,
    /// Version of the policy that produced the decision.
    pub policy_version: String,
    /// Workspace trust/config revision bound into the approval decision.
    pub context_revision: chrono::DateTime<chrono::Utc>,
    /// Current durable state.
    pub status: ToolInvocationStatus,
    /// Linked approval request for `WaitingApproval` invocations.
    pub approval_request_id: Option<ApprovalRequestId>,
    /// Successful durable result.
    #[ts(type = "any")]
    pub result: Option<serde_json::Value>,
    /// Safe failure summary.
    pub error: Option<String>,
    /// Opaque recovery metadata such as file pre/post hashes.
    #[ts(type = "any")]
    pub recovery: Option<serde_json::Value>,
    /// Worker lease token while executing.
    pub lease_token: Option<uuid::Uuid>,
    /// Number of successful claims.
    pub attempts: u32,
    /// Cancellation requested after execution had already been claimed.
    pub cancel_requested: bool,
    /// Correlation id propagated from [`ExecutionContext`].
    pub correlation_id: uuid::Uuid,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last durable state update.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// First execution claim timestamp.
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Terminal completion timestamp.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A persisted event in a run's timeline.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunEvent {
    /// Database row id.
    pub id: i64,
    /// Run the event belongs to.
    pub run_id: AgentRunId,
    /// Thread the event belongs to.
    pub thread_id: ThreadId,
    /// Sequence order inside the run.
    pub sequence: i64,
    /// Event type tag.
    pub event_type: String,
    /// Structured payload.
    #[ts(type = "any")]
    pub payload: serde_json::Value,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// An artifact produced by an agent run.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Artifact {
    /// Database row id.
    pub id: i64,
    /// Run that produced the artifact.
    pub run_id: AgentRunId,
    /// Artifact name.
    pub name: String,
    /// MIME type.
    pub mime_type: String,
    /// Optional filesystem path.
    pub path: Option<String>,
    /// Optional content preview.
    pub content_preview: Option<String>,
}

/// Unique identifier for a persisted approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApprovalRequestId(pub i64);

/// Lifecycle status of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ApprovalRequestStatus {
    /// Awaiting user decision.
    Pending,
    /// User approved the action.
    Approved,
    /// User denied the action.
    Denied,
}

impl ApprovalRequestStatus {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Approved => "Approved",
            Self::Denied => "Denied",
        }
    }
}

impl TryFrom<&str> for ApprovalRequestStatus {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Pending" => Ok(Self::Pending),
            "Approved" => Ok(Self::Approved),
            "Denied" => Ok(Self::Denied),
            _ => Err(AppError::Internal {
                message: format!("unknown approval request status: {value}"),
            }),
        }
    }
}

/// A request for user approval of an action.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApprovalRequest {
    /// Database row id.
    pub id: ApprovalRequestId,
    /// Run that requested approval.
    pub run_id: AgentRunId,
    /// Workspace that owns the run.
    pub workspace_id: WorkspaceId,
    /// Thread that owns the run.
    pub thread_id: ThreadId,
    /// Action being requested.
    pub action: String,
    /// Resource the action targets.
    pub resource: String,
    /// Current status.
    pub status: ApprovalRequestStatus,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Resolution timestamp.
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Optional reason provided when the request was denied.
    pub resolution_reason: Option<String>,
}

/// Unique identifier for a model provider configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderId(pub uuid::Uuid);

impl ProviderId {
    /// Create a new random provider identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for ProviderId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a model definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelId(pub uuid::Uuid);

impl ModelId {
    /// Create a new random model identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for ModelId {
    fn default() -> Self {
        Self::new()
    }
}

/// Supported provider backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ProviderKind {
    OpenAI,
    Anthropic,
    Moonshot,
    DeepSeek,
    Google,
    Groq,
    OpenRouter,
    AzureOpenAI,
    Ollama,
    Custom,
}

impl ProviderKind {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Moonshot => "Moonshot",
            Self::DeepSeek => "DeepSeek",
            Self::Google => "Google",
            Self::Groq => "Groq",
            Self::OpenRouter => "OpenRouter",
            Self::AzureOpenAI => "AzureOpenAI",
            Self::Ollama => "Ollama",
            Self::Custom => "Custom",
        }
    }
}

impl TryFrom<&str> for ProviderKind {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "OpenAI" => Ok(Self::OpenAI),
            "Anthropic" => Ok(Self::Anthropic),
            "Moonshot" => Ok(Self::Moonshot),
            "DeepSeek" => Ok(Self::DeepSeek),
            "Google" => Ok(Self::Google),
            "Groq" => Ok(Self::Groq),
            "OpenRouter" => Ok(Self::OpenRouter),
            "AzureOpenAI" => Ok(Self::AzureOpenAI),
            "Ollama" => Ok(Self::Ollama),
            "Custom" => Ok(Self::Custom),
            _ => Err(AppError::Internal {
                message: format!("unknown provider kind: {value}"),
            }),
        }
    }
}

/// Retry policy for provider API calls.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            initial_backoff_ms: 500,
            max_backoff_ms: 8_000,
        }
    }
}

/// Persisted configuration for a model provider.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderConfig {
    pub id: ProviderId,
    pub kind: ProviderKind,
    pub display_name: String,
    pub base_url: Option<String>,
    pub api_key_reference: String,
    pub organization_id: Option<String>,
    pub project_id: Option<String>,
    pub default_headers: std::collections::HashMap<String, String>,
    pub timeout_ms: u64,
    pub retry_policy: RetryPolicy,
    pub fallback_provider_ids: Vec<ProviderId>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Capability matrix for a specific model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export)]
#[allow(clippy::struct_excessive_bools)]
pub struct ModelCapability {
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_json_schema: bool,
    pub supports_vision: bool,
    pub supports_pdf: bool,
    pub supports_system_prompt: bool,
    pub supports_embeddings: bool,
    pub max_context_tokens: Option<u64>,
    pub input_price_per_1k: Option<f64>,
    pub output_price_per_1k: Option<f64>,
}

/// Model definition registered under a provider.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelInfo {
    pub id: ModelId,
    pub provider_id: ProviderId,
    pub provider_name: String,
    pub model_name: String,
    pub display_name: String,
    pub capabilities: ModelCapability,
}

/// Scope that owns an active provider/model selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ModelSelectionScope {
    /// Fallback used when no narrower selection exists.
    Global,
    /// Default for all threads in one workspace.
    Workspace,
    /// Selection for one conversation thread.
    Thread,
}

impl ModelSelectionScope {
    /// Stable persistence representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Workspace => "Workspace",
            Self::Thread => "Thread",
        }
    }
}

impl TryFrom<&str> for ModelSelectionScope {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Global" => Ok(Self::Global),
            "Workspace" => Ok(Self::Workspace),
            "Thread" => Ok(Self::Thread),
            _ => Err(AppError::Internal {
                message: format!("unknown model selection scope: {value}"),
            }),
        }
    }
}

/// Persisted active provider/model selection at a global, workspace, or thread scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActiveModelSelection {
    pub scope: ModelSelectionScope,
    pub workspace_id: Option<WorkspaceId>,
    pub thread_id: Option<ThreadId>,
    pub provider_id: ProviderId,
    pub model_id: ModelId,
    pub provider_name: String,
    pub model_name: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Result of the most recent bounded provider health check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ProviderHealthStatus {
    Checking,
    Ready,
    Degraded,
    InvalidCredentials,
    Unsupported,
}

impl ProviderHealthStatus {
    /// Stable persistence representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Checking => "Checking",
            Self::Ready => "Ready",
            Self::Degraded => "Degraded",
            Self::InvalidCredentials => "InvalidCredentials",
            Self::Unsupported => "Unsupported",
        }
    }
}

impl TryFrom<&str> for ProviderHealthStatus {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Checking" => Ok(Self::Checking),
            "Ready" => Ok(Self::Ready),
            "Degraded" => Ok(Self::Degraded),
            "InvalidCredentials" => Ok(Self::InvalidCredentials),
            "Unsupported" => Ok(Self::Unsupported),
            _ => Err(AppError::Internal {
                message: format!("unknown provider health status: {value}"),
            }),
        }
    }
}

/// Safe, persisted provider health summary. It never contains credentials or raw responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderHealth {
    pub provider_id: ProviderId,
    pub model_id: ModelId,
    pub status: ProviderHealthStatus,
    pub error_code: Option<String>,
    pub message: Option<String>,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

/// Immutable provider/model configuration captured for a run before network execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunModelSnapshot {
    pub run_id: AgentRunId,
    pub provider_id: ProviderId,
    pub model_id: ModelId,
    pub provider_name: String,
    pub model_name: String,
    pub provider_config_updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// App-level usage budget guard.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UsageBudget {
    pub per_run_usd: Option<f64>,
    pub daily_usd: Option<f64>,
}

impl Default for UsageBudget {
    fn default() -> Self {
        Self {
            per_run_usd: Some(1.0),
            daily_usd: Some(10.0),
        }
    }
}

/// Recorded LLM usage for a single run/model call.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UsageRecord {
    pub id: i64,
    pub run_id: AgentRunId,
    pub provider_id: ProviderId,
    pub model_id: ModelId,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Unique identifier for a persisted memory item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MemoryId(pub uuid::Uuid);

impl MemoryId {
    /// Create a new random memory identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

/// Scope at which a memory applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum MemoryScope {
    /// Tied to the current application session.
    Session,
    /// Tied to a conversation thread.
    Thread,
    /// Tied to a workspace.
    Workspace,
    /// Tied to the user profile.
    User,
}

impl MemoryScope {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Thread => "Thread",
            Self::Workspace => "Workspace",
            Self::User => "User",
        }
    }
}

impl TryFrom<&str> for MemoryScope {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Session" => Ok(Self::Session),
            "Thread" => Ok(Self::Thread),
            "Workspace" => Ok(Self::Workspace),
            "User" => Ok(Self::User),
            _ => Err(AppError::Internal {
                message: format!("unknown memory scope: {value}"),
            }),
        }
    }
}

/// A single persisted memory entry.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MemoryItem {
    pub id: MemoryId,
    pub scope: MemoryScope,
    pub workspace_id: Option<WorkspaceId>,
    pub thread_id: Option<ThreadId>,
    pub key: String,
    pub value: String,
    pub sensitive: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Scope at which a permission decision applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum PermissionScope {
    /// Apply to a single request.
    Once,
    /// Apply for the remainder of the current run.
    Run,
    /// Apply for the remainder of the current thread.
    Thread,
    /// Apply for the remainder of the current workspace.
    Workspace,
    /// Apply globally until revoked.
    Global,
}

impl PermissionScope {
    /// String representation used for persistence and serialization.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Once => "Once",
            Self::Run => "Run",
            Self::Thread => "Thread",
            Self::Workspace => "Workspace",
            Self::Global => "Global",
        }
    }

    /// Numeric rank where narrower scopes have lower values.
    #[must_use]
    pub const fn rank(&self) -> u8 {
        match self {
            Self::Once => 0,
            Self::Run => 1,
            Self::Thread => 2,
            Self::Workspace => 3,
            Self::Global => 4,
        }
    }

    /// Whether this scope is no broader than `other`.
    #[must_use]
    pub const fn is_at_most(&self, other: Self) -> bool {
        self.rank() <= other.rank()
    }
}

impl TryFrom<&str> for PermissionScope {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Once" => Ok(Self::Once),
            "Run" => Ok(Self::Run),
            "Thread" => Ok(Self::Thread),
            "Workspace" => Ok(Self::Workspace),
            "Global" => Ok(Self::Global),
            _ => Err(AppError::Internal {
                message: format!("unknown permission scope: {value}"),
            }),
        }
    }
}

/// Built-in agent roles available to the orchestrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum BuiltInAgent {
    /// General-purpose fallback agent.
    Default,
    /// Explores codebases, files, and available tools.
    Explorer,
    /// Breaks tasks into plans and sub-tasks.
    Planner,
    /// Writes code and makes filesystem changes.
    Worker,
    /// Reviews code quality and correctness.
    Reviewer,
    /// Focuses on security review.
    SecurityReviewer,
    /// Runs tests and validates behavior.
    Tester,
    /// Researches external information and context.
    Researcher,
    /// Writes documentation.
    DocWriter,
}

impl BuiltInAgent {
    /// String representation used for registry lookups and persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Explorer => "explorer",
            Self::Planner => "planner",
            Self::Worker => "worker",
            Self::Reviewer => "reviewer",
            Self::SecurityReviewer => "security-reviewer",
            Self::Tester => "tester",
            Self::Researcher => "researcher",
            Self::DocWriter => "doc-writer",
        }
    }
}

impl std::fmt::Display for BuiltInAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Definition of an agent that can be registered with the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentDefinition {
    /// Machine-readable agent name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// System instructions provided to the agent.
    pub system_instructions: String,
    /// Names of tools the agent is allowed to use.
    pub allowed_tools: Vec<String>,
    /// Default model selection policy.
    pub default_model_policy: String,
    /// Default permission scope for the agent.
    pub default_permission_scope: PermissionScope,
}

/// A single subagent run managed by the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubagentRun {
    /// Subagent run identifier.
    pub id: AgentRunId,
    /// Parent run that owns this subagent.
    pub parent_run_id: AgentRunId,
    /// Name of the agent definition used.
    pub agent_name: String,
    /// Current lifecycle status.
    pub status: AgentRunStatus,
    /// Task description passed to the subagent.
    pub task_description: String,
    /// Short summary of the subagent output.
    pub output_summary: Option<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Completion timestamp.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A plan describing the subagents to run for a parent run.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OrchestrationPlan {
    /// Parent run the plan belongs to.
    pub parent_run_id: AgentRunId,
    /// Subagents to execute.
    pub subagents: Vec<SubagentRun>,
    /// Pattern ids that conditioned this plan (loose coupling evidence).
    #[serde(default)]
    pub pattern_ids: Vec<WorkflowPatternId>,
    /// Human-readable reason the plan was shaped this way.
    #[serde(default)]
    pub planning_rationale: String,
}

/// Identifier for a learned workflow pattern in the memory layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkflowPatternId(pub uuid::Uuid);

impl WorkflowPatternId {
    /// Create a new random pattern id.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for WorkflowPatternId {
    fn default() -> Self {
        Self::new()
    }
}

/// Lifecycle status of a workflow pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum WorkflowPatternStatus {
    /// Eligible for recall during planning.
    Active,
    /// Proposed by the system; not yet trusted by the user.
    Suggested,
    /// Explicitly silenced by the user.
    Muted,
}

impl WorkflowPatternStatus {
    /// Persistable string form.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suggested => "suggested",
            Self::Muted => "muted",
        }
    }
}

impl TryFrom<&str> for WorkflowPatternStatus {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "active" => Ok(Self::Active),
            "suggested" => Ok(Self::Suggested),
            "muted" => Ok(Self::Muted),
            _ => Err(AppError::Internal {
                message: format!("unknown workflow pattern status: {value}"),
            }),
        }
    }
}

/// A durable user/workspace work habit used to condition multi-agent planning.
///
/// Patterns are owned by the memory layer. Orchestration only consumes DTO-like
/// hints through ports so the two modules can evolve independently.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkflowPattern {
    pub id: WorkflowPatternId,
    pub scope: MemoryScope,
    pub workspace_id: Option<WorkspaceId>,
    pub name: String,
    pub summary: String,
    /// Free-text triggers matched during recall (keywords, phrases).
    pub trigger_text: String,
    /// Preferred agent role names, ordered.
    pub preferred_roles: Vec<String>,
    pub collaboration_style: String,
    pub strength: f64,
    pub success_count: i64,
    pub failure_count: i64,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: WorkflowPatternStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Lightweight pattern hint returned across module boundaries (no storage coupling).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PatternHint {
    pub id: WorkflowPatternId,
    pub name: String,
    pub summary: String,
    pub preferred_roles: Vec<String>,
    pub collaboration_style: String,
    pub strength: f64,
    pub score: f64,
}

/// Identifier for one multi-agent orchestration session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OrchestrationId(pub uuid::Uuid);

impl OrchestrationId {
    /// Create a new random orchestration id.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for OrchestrationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Lifecycle of a multi-agent orchestration session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum OrchestrationStatus {
    Planning,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl OrchestrationStatus {
    /// Persistable string form.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Planning => "Planning",
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

impl TryFrom<&str> for OrchestrationStatus {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Planning" => Ok(Self::Planning),
            "Running" => Ok(Self::Running),
            "Completed" => Ok(Self::Completed),
            "Failed" => Ok(Self::Failed),
            "Cancelled" => Ok(Self::Cancelled),
            _ => Err(AppError::Internal {
                message: format!("unknown orchestration status: {value}"),
            }),
        }
    }
}

/// Durable multi-agent orchestration session.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Orchestration {
    pub id: OrchestrationId,
    pub parent_run_id: AgentRunId,
    pub workspace_id: WorkspaceId,
    pub thread_id: ThreadId,
    pub task: String,
    pub status: OrchestrationStatus,
    pub plan: OrchestrationPlan,
    pub pattern_ids: Vec<WorkflowPatternId>,
    pub result_summary: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Outcome snapshot used by the memory layer to learn without importing workflows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationOutcome {
    pub workspace_id: WorkspaceId,
    pub task: String,
    pub success: bool,
    pub agent_names: Vec<String>,
    pub pattern_ids: Vec<WorkflowPatternId>,
    pub result_summary: Option<String>,
}

/// A loaded instruction file (e.g. `AGENTS.md`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InstructionFile {
    pub path: String,
    pub content: String,
    pub scope: String,
}

/// A chunk retrieved from the RAG vector index.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RagChunk {
    pub id: i64,
    pub document_path: String,
    pub chunk_index: usize,
    pub content: String,
    pub score: f64,
}

/// Summary of everything assembled for a run's prompt context.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContextSummary {
    pub run_id: AgentRunId,
    pub thread_id: ThreadId,
    pub instructions: Vec<InstructionFile>,
    pub memories: Vec<MemoryItem>,
    pub rag_chunks: Vec<RagChunk>,
    pub estimated_tokens: u64,
    pub privacy_flags: Vec<String>,
}

/// Backend-authoritative product capability probe (single source of truth for UI).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[allow(clippy::struct_excessive_bools)]
pub struct FeatureCapabilities {
    /// Durable single-agent conversation + safe tools + approval recovery.
    pub core_agent_workflow: bool,
    /// Multi-role orchestration secondary path (durable sessions).
    pub multi_agent_orchestration: bool,
    /// Project instructions, non-sensitive memory, and RAG enter the agent prompt.
    pub context_injection: bool,
    /// Workspace disk scan / rebuild for RAG.
    pub workspace_indexer: bool,
    /// Search + structured edit tools in the durable tool allowlist.
    pub advanced_file_tools: bool,
    /// Skill invocation inside agent runs (list-only when false).
    pub skill_invocation: bool,
    /// Native browser/desktop automation.
    pub native_automation: bool,
    /// MCP servers attached to agent tool loop.
    pub mcp_agent_tools: bool,
    /// Terminal / shell execution.
    pub terminal: bool,
    /// Git mutation (stage/commit/push).
    pub git_mutation: bool,
    /// Scheduled automations (UI + IPC).
    pub automations: bool,
    /// Human-readable notes for disabled surfaces.
    pub notes: Vec<String>,
}

impl Default for FeatureCapabilities {
    fn default() -> Self {
        Self {
            core_agent_workflow: true,
            multi_agent_orchestration: true,
            context_injection: true,
            workspace_indexer: true,
            advanced_file_tools: true,
            skill_invocation: false,
            native_automation: false,
            mcp_agent_tools: false,
            terminal: false,
            git_mutation: false,
            automations: false,
            notes: vec![
                "Default path: single agent + tools (fs_list/read/search/write/edit, git status/diff)."
                    .to_owned(),
                "Multi-agent is opt-in production secondary path with durable sessions.".to_owned(),
                "Terminal, Git mutation, MCP agent tools, Browser/Desktop, Automations remain closed."
                    .to_owned(),
            ],
        }
    }
}

/// Unique identifier for a Portico plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginId(pub uuid::Uuid);

impl PluginId {
    /// Create a new random plugin identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for PluginId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a skill exposed by a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillId(pub uuid::Uuid);

impl SkillId {
    /// Create a new random skill identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for SkillId {
    fn default() -> Self {
        Self::new()
    }
}

/// Permissions declared by a plugin at install time.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginPermissions {
    /// Host patterns the plugin is allowed to access over the network.
    pub network: Vec<String>,
    /// Filesystem access level: `"none"`, `"read"`, or `"write"`.
    pub filesystem: String,
}

/// A host capability implemented by a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum PluginCapability {
    /// Render Markdown content for preview.
    #[serde(rename = "markdown.preview")]
    #[ts(rename = "markdown.preview")]
    MarkdownPreview,
    /// Export Markdown content as HTML.
    #[serde(rename = "markdown.export.html")]
    #[ts(rename = "markdown.export.html")]
    MarkdownExportHtml,
    /// Export Markdown content as DOCX.
    #[serde(rename = "markdown.export.docx")]
    #[ts(rename = "markdown.export.docx")]
    MarkdownExportDocx,
    /// Export Markdown content as PDF.
    #[serde(rename = "markdown.export.pdf")]
    #[ts(rename = "markdown.export.pdf")]
    MarkdownExportPdf,
}

/// Manifest describing an installed Portico plugin.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginManifest {
    /// Plugin identifier.
    pub id: PluginId,
    /// Short machine-readable name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Long-form description.
    pub description: String,
    /// Names of skills advertised by the plugin.
    pub skills: Vec<String>,
    /// Names of tools provided by the plugin.
    pub tools: Vec<String>,
    /// Plugin-relative UI entrypoint, when the plugin provides a host view.
    #[serde(default)]
    pub entrypoint: Option<String>,
    /// Host capabilities implemented by this plugin.
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
    /// Absolute path containing the installed plugin files.
    #[serde(default)]
    pub install_path: Option<String>,
    /// Declared permissions.
    pub permissions: PluginPermissions,
    /// Whether the plugin is currently enabled.
    pub enabled: bool,
    /// Timestamp when the plugin was installed.
    pub installed_at: chrono::DateTime<chrono::Utc>,
}

/// A skill exposed by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Skill {
    /// Skill identifier.
    pub id: SkillId,
    /// Owning plugin.
    pub plugin_id: PluginId,
    /// Short machine-readable name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Description used to match natural-language triggers.
    pub trigger_description: String,
    /// Optional path to an instruction file bundled with the plugin.
    pub instruction_file: Option<String>,
    /// Names of tools the skill requires.
    pub required_tools: Vec<String>,
}

/// Transport protocol used to communicate with an MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum McpTransport {
    /// Standard input/output.
    Stdio,
    /// HTTP/SSE.
    Http,
}

impl McpTransport {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "Stdio",
            Self::Http => "Http",
        }
    }
}

impl TryFrom<&str> for McpTransport {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Stdio" => Ok(Self::Stdio),
            "Http" => Ok(Self::Http),
            _ => Err(AppError::Internal {
                message: format!("unknown MCP transport: {value}"),
            }),
        }
    }
}

/// Persisted configuration for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpServerConfig {
    /// Database row id.
    pub id: i64,
    /// Human-readable name.
    pub name: String,
    /// Transport protocol.
    pub transport: McpTransport,
    /// Command to execute for stdio transport.
    pub command: Option<String>,
    /// Arguments for the command.
    pub args: Vec<String>,
    /// URL for HTTP transport.
    pub url: Option<String>,
    /// Environment variables for the server process.
    pub env: std::collections::HashMap<String, String>,
    /// Whether the server is enabled.
    pub enabled: bool,
}

/// Unique identifier for a background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackgroundTaskId(pub uuid::Uuid);

impl BackgroundTaskId {
    /// Create a new random background task identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for BackgroundTaskId {
    fn default() -> Self {
        Self::new()
    }
}

/// Lifecycle status of a background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum BackgroundTaskStatus {
    /// Task is queued and waiting for a worker.
    Queued,
    /// Task is currently being processed.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed and will not be retried.
    Failed,
    /// Task was cancelled before completion.
    Cancelled,
}

impl BackgroundTaskStatus {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }

    /// Whether the status represents a finished task.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

impl TryFrom<&str> for BackgroundTaskStatus {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Queued" => Ok(Self::Queued),
            "Running" => Ok(Self::Running),
            "Completed" => Ok(Self::Completed),
            "Failed" => Ok(Self::Failed),
            "Cancelled" => Ok(Self::Cancelled),
            _ => Err(AppError::Internal {
                message: format!("unknown background task status: {value}"),
            }),
        }
    }
}

/// Kind of work a background task represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum TaskKind {
    /// An autonomous agent run.
    AgentRun,
    /// A recurring or one-off routine.
    Routine,
    /// A scheduled wakeup for a thread.
    ThreadWakeup,
    /// A job spawned by an automation.
    ScheduledJob,
}

impl TaskKind {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AgentRun => "AgentRun",
            Self::Routine => "Routine",
            Self::ThreadWakeup => "ThreadWakeup",
            Self::ScheduledJob => "ScheduledJob",
        }
    }
}

impl TryFrom<&str> for TaskKind {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "AgentRun" => Ok(Self::AgentRun),
            "Routine" => Ok(Self::Routine),
            "ThreadWakeup" => Ok(Self::ThreadWakeup),
            "ScheduledJob" => Ok(Self::ScheduledJob),
            _ => Err(AppError::Internal {
                message: format!("unknown task kind: {value}"),
            }),
        }
    }
}

/// A unit of background work queued for asynchronous execution.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackgroundTask {
    /// Task identifier.
    pub id: BackgroundTaskId,
    /// Owning workspace.
    pub workspace_id: WorkspaceId,
    /// Optional owning thread.
    pub thread_id: Option<ThreadId>,
    /// Optional owning run.
    pub run_id: Option<AgentRunId>,
    /// Kind of work to perform.
    pub task_kind: TaskKind,
    /// Opaque task payload.
    #[ts(type = "any")]
    pub payload: serde_json::Value,
    /// Current lifecycle status.
    pub status: BackgroundTaskStatus,
    /// Higher values are processed first.
    pub priority: i64,
    /// Number of processing attempts so far.
    pub attempts: u32,
    /// Maximum number of attempts before marking failed.
    pub max_attempts: u32,
    /// Earliest time the task should be processed.
    pub scheduled_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp when the task was leased.
    pub leased_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Worker identifier that currently holds the lease.
    pub leased_by: Option<String>,
    /// Earliest time a failed task should be retried.
    pub next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Last error message if the task failed.
    pub error_message: Option<String>,
    /// Short summary of the task result.
    pub result_summary: Option<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Unique identifier for an automation rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AutomationId(pub uuid::Uuid);

impl AutomationId {
    /// Create a new random automation identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for AutomationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Trigger that can activate an automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum AutomationTrigger {
    /// Activated on a cron schedule.
    Scheduled,
    /// Activated by a filesystem change.
    FileChange,
    /// Activated by a git event.
    GitEvent,
    /// Activated manually by the user.
    ManualRoutine,
    /// Reserved for future webhook triggers.
    WebhookReserved,
    /// Activated by a thread wakeup event.
    ThreadWakeup,
}

impl AutomationTrigger {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Scheduled => "Scheduled",
            Self::FileChange => "FileChange",
            Self::GitEvent => "GitEvent",
            Self::ManualRoutine => "ManualRoutine",
            Self::WebhookReserved => "WebhookReserved",
            Self::ThreadWakeup => "ThreadWakeup",
        }
    }
}

impl TryFrom<&str> for AutomationTrigger {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Scheduled" => Ok(Self::Scheduled),
            "FileChange" => Ok(Self::FileChange),
            "GitEvent" => Ok(Self::GitEvent),
            "ManualRoutine" => Ok(Self::ManualRoutine),
            "WebhookReserved" => Ok(Self::WebhookReserved),
            "ThreadWakeup" => Ok(Self::ThreadWakeup),
            _ => Err(AppError::Internal {
                message: format!("unknown automation trigger: {value}"),
            }),
        }
    }
}

/// A user-defined automation rule.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Automation {
    /// Automation identifier.
    pub id: AutomationId,
    /// Owning workspace.
    pub workspace_id: WorkspaceId,
    /// Display name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Trigger kind.
    pub trigger: AutomationTrigger,
    /// Cron expression for scheduled triggers.
    pub cron_expr: Option<String>,
    /// Whether the automation is currently enabled.
    pub enabled: bool,
    /// Permission policy configuration.
    #[ts(type = "any")]
    pub permission_policy: serde_json::Value,
    /// Next scheduled run time.
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Last time the automation ran.
    pub last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Unique identifier for a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NotificationId(pub uuid::Uuid);

impl NotificationId {
    /// Create a new random notification identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for NotificationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Category of a user-facing notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum NotificationCategory {
    /// System-level message.
    System,
    /// In-app message.
    InApp,
    /// A user approval is required.
    ApprovalRequired,
    /// A task finished processing.
    TaskCompleted,
}

impl NotificationCategory {
    /// String representation used for persistence.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::System => "System",
            Self::InApp => "InApp",
            Self::ApprovalRequired => "ApprovalRequired",
            Self::TaskCompleted => "TaskCompleted",
        }
    }
}

impl TryFrom<&str> for NotificationCategory {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "System" => Ok(Self::System),
            "InApp" => Ok(Self::InApp),
            "ApprovalRequired" => Ok(Self::ApprovalRequired),
            "TaskCompleted" => Ok(Self::TaskCompleted),
            _ => Err(AppError::Internal {
                message: format!("unknown notification category: {value}"),
            }),
        }
    }
}

/// A user-facing notification persisted in the notification center.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notification {
    /// Notification identifier.
    pub id: NotificationId,
    /// Owning workspace.
    pub workspace_id: WorkspaceId,
    /// Optional owning thread.
    pub thread_id: Option<ThreadId>,
    /// Optional owning run.
    pub run_id: Option<AgentRunId>,
    /// Notification title.
    pub title: String,
    /// Notification body.
    pub body: String,
    /// Notification category.
    pub category: NotificationCategory,
    /// Whether the notification has been read.
    pub read: bool,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Unique identifier for an in-app browser window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrowserWindowId(pub String);

/// Metadata describing an open in-app browser window.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrowserWindowInfo {
    /// Window identifier.
    pub id: BrowserWindowId,
    /// Currently loaded URL.
    pub url: String,
    /// Window title.
    pub title: String,
}

/// An action executed inside an in-app browser window.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum BrowserAction {
    /// Click the first element matching `selector`.
    Click {
        /// CSS selector for the target element.
        selector: String,
    },
    /// Type text into the first element matching `selector`.
    Type {
        /// CSS selector for the target input.
        selector: String,
        /// Text to type.
        text: String,
    },
    /// Return the visible text content of the page.
    ExtractVisibleText,
    /// Wait for the given number of milliseconds.
    Wait {
        /// Duration to wait.
        ms: u64,
    },
    /// Capture a screenshot of the browser window.
    Screenshot,
}

/// A captured desktop screenshot encoded as a PNG.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesktopCapture {
    /// PNG image encoded as base64.
    pub image_base64: String,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

/// Preview of an artifact file suitable for frontend rendering.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArtifactPreview {
    /// Original filesystem path.
    pub path: String,
    /// Detected MIME type.
    pub mime_type: String,
    /// File contents encoded as base64.
    pub content_base64: String,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Unique identifier for a collected diagnostics bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiagnosticsBundleId(pub uuid::Uuid);

impl DiagnosticsBundleId {
    /// Create a new random diagnostics bundle identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for DiagnosticsBundleId {
    fn default() -> Self {
        Self::new()
    }
}

/// A redacted diagnostics bundle collected for support or debugging.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiagnosticsBundle {
    /// Bundle identifier.
    pub id: DiagnosticsBundleId,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Path to the safe log-omission notice inside the bundle.
    pub log_path: String,
    /// Path to the aggregate-only audit summary inside the bundle.
    pub audit_summary_path: String,
    /// Application version at the time the bundle was created.
    pub app_version: String,
    /// Operating system information.
    pub os_info: String,
    /// Whether sensitive payloads were omitted from the bundle contents.
    pub redacted: bool,
    /// Total size of the bundle directory in bytes.
    pub size_bytes: u64,
}

/// A tracked database migration applied by sqlx.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MigrationInfo {
    /// Migration version number.
    pub version: i64,
    /// Migration name / description.
    pub name: String,
    /// Timestamp when the migration was applied.
    pub applied_at: chrono::DateTime<chrono::Utc>,
    /// Migration checksum.
    pub checksum: String,
}

/// Product-level error type returned by Portico APIs.
#[derive(Debug, thiserror::Error, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum AppError {
    /// A requested resource was not found.
    #[error("not found: {resource}")]
    NotFound {
        /// Description of the missing resource.
        resource: String,
    },
    /// The operation was rejected for security or policy reasons.
    #[error("permission denied: {reason}")]
    PermissionDenied {
        /// Human-readable reason for the denial.
        reason: String,
    },
    /// A generic internal error.
    #[error("internal error: {message}")]
    Internal {
        /// Error message.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_id_is_unique() {
        let first = WorkspaceId::new();
        let second = WorkspaceId::new();
        assert_ne!(first, second);
    }

    #[test]
    fn app_error_serializes() {
        let err = AppError::NotFound {
            resource: "workspace".to_owned(),
        };
        let json = serde_json::to_string(&err).expect("should serialize");
        assert!(json.contains("workspace"));
    }

    #[test]
    fn agent_run_status_roundtrips() {
        let status = AgentRunStatus::WaitingApproval;
        let json = serde_json::to_string(&status).expect("should serialize");
        assert_eq!(json, "\"WaitingApproval\"");
    }
}
