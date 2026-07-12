//! Durable approval decisions and single-claim execution grants.

use crate::Storage;
use app_models::{
    AppError, ApprovalRequest, ApprovalRequestId, ApprovalRequestStatus, ToolInvocation,
    ToolInvocationId, ToolInvocationStatus,
};
use serde_json::Value;
use std::sync::Arc;

/// Unforgeable lease returned only after an invocation is atomically claimed.
#[derive(Debug)]
pub struct ExecutionGrant {
    invocation: ToolInvocation,
}

impl ExecutionGrant {
    /// Durable invocation authorized for execution.
    #[must_use]
    pub const fn invocation(&self) -> &ToolInvocation {
        &self.invocation
    }
}

/// Result of an idempotent approval call.
#[derive(Debug)]
pub enum ApprovalExecutionOutcome {
    /// This caller won the execution claim and must complete or fail it.
    Granted(ExecutionGrant),
    /// The same approval was already claimed or reached a final state.
    AlreadyHandled(ToolInvocation),
}

/// Owns approval resolution and invocation lease transitions.
#[derive(Clone)]
pub struct ApprovalBroker {
    storage: Arc<dyn Storage>,
}

impl ApprovalBroker {
    /// Create a broker over durable storage.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Approve and attempt to claim the linked invocation exactly once.
    ///
    /// Repeating an already-approved request is idempotent. Exactly one caller
    /// receives a grant; all others receive the current durable invocation.
    ///
    /// # Errors
    ///
    /// Rejects missing approvals, conflicting decisions, stale workspace
    /// revisions, or invalid state transitions.
    pub async fn approve(
        &self,
        id: ApprovalRequestId,
    ) -> Result<ApprovalExecutionOutcome, AppError> {
        let (_, invocation) = self
            .storage
            .resolve_tool_approval(id, ApprovalRequestStatus::Approved, None)
            .await?;
        if invocation.status != ToolInvocationStatus::Approved {
            return Ok(ApprovalExecutionOutcome::AlreadyHandled(invocation));
        }
        match self.storage.claim_tool_invocation(invocation.id).await {
            Ok(invocation) => Ok(ApprovalExecutionOutcome::Granted(ExecutionGrant {
                invocation,
            })),
            Err(AppError::PermissionDenied { .. }) => Ok(ApprovalExecutionOutcome::AlreadyHandled(
                self.storage.get_tool_invocation(invocation.id).await?,
            )),
            Err(error) => Err(error),
        }
    }

    /// Claim a policy-allowed invocation exactly once.
    ///
    /// # Errors
    ///
    /// Rejects stale or non-ready invocations.
    pub async fn claim_ready(&self, id: ToolInvocationId) -> Result<ExecutionGrant, AppError> {
        let invocation = self.storage.claim_tool_invocation(id).await?;
        Ok(ExecutionGrant { invocation })
    }

    /// Deny a linked invocation before it can execute.
    ///
    /// # Errors
    ///
    /// Rejects a conflicting prior approval or an invalid state transition.
    pub async fn deny(
        &self,
        id: ApprovalRequestId,
        reason: Option<&str>,
    ) -> Result<(ApprovalRequest, ToolInvocation), AppError> {
        self.storage
            .resolve_tool_approval(id, ApprovalRequestStatus::Denied, reason)
            .await
    }

    /// Commit a successful execution receipt using the grant lease.
    ///
    /// # Errors
    ///
    /// Rejects stale, duplicated, or forged leases.
    pub async fn complete(
        &self,
        grant: ExecutionGrant,
        result: Value,
    ) -> Result<ToolInvocation, AppError> {
        let lease = grant.invocation.lease_token.ok_or_else(|| AppError::PermissionDenied {
            reason: "execution grant has no lease".to_owned(),
        })?;
        self.storage.complete_tool_invocation(grant.invocation.id, lease, result).await
    }

    /// Commit a failed execution receipt using the grant lease.
    ///
    /// # Errors
    ///
    /// Rejects stale, duplicated, or forged leases.
    pub async fn fail(
        &self,
        grant: ExecutionGrant,
        error: &str,
    ) -> Result<ToolInvocation, AppError> {
        let lease = grant.invocation.lease_token.ok_or_else(|| AppError::PermissionDenied {
            reason: "execution grant has no lease".to_owned(),
        })?;
        self.storage.fail_tool_invocation(grant.invocation.id, lease, error).await
    }

    /// Complete an invocation whose post-image proves the effect happened.
    ///
    /// # Errors
    ///
    /// Rejects invocations not currently awaiting reconciliation.
    pub async fn complete_reconciled(
        &self,
        id: ToolInvocationId,
        result: Value,
    ) -> Result<ToolInvocation, AppError> {
        self.storage.complete_reconciled_tool_invocation(id, result).await
    }

    /// Requeue an invocation whose pre-image proves the effect did not happen.
    ///
    /// # Errors
    ///
    /// Rejects stale context or invocations not awaiting reconciliation.
    pub async fn requeue_reconciled(
        &self,
        id: ToolInvocationId,
    ) -> Result<ToolInvocation, AppError> {
        self.storage.requeue_reconciled_tool_invocation(id).await
    }

    /// Fail an invocation when reconciliation finds an external-state conflict.
    ///
    /// # Errors
    ///
    /// Rejects invocations not currently awaiting reconciliation.
    pub async fn fail_reconciled(
        &self,
        id: ToolInvocationId,
        error: &str,
    ) -> Result<ToolInvocation, AppError> {
        self.storage.fail_reconciled_tool_invocation(id, error).await
    }

    /// Recover abandoned executing invocations into reconciliation-required.
    ///
    /// # Errors
    ///
    /// Returns an error when recovery persistence fails.
    pub async fn recover(&self) -> Result<u64, AppError> {
        self.storage.recover_tool_invocations().await
    }
}
