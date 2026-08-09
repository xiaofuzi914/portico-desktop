//! Outcome evaluation: technical completion ≠ user-goal success.

use app_models::{
    AgentRunStatus, OutcomeSignal, RunFeedback, RunFeedbackRating, ToolUsageSummary,
};

/// Weights for composing a multi-signal outcome score.
#[derive(Debug, Clone, Copy)]
pub struct OutcomeWeights {
    pub technical_completion: f64,
    pub artifact_or_test_success: f64,
    pub explicit_user_feedback: f64,
    pub no_immediate_correction: f64,
    pub low_tool_failure_rate: f64,
}

impl Default for OutcomeWeights {
    fn default() -> Self {
        Self {
            technical_completion: 0.25,
            artifact_or_test_success: 0.30,
            explicit_user_feedback: 0.30,
            no_immediate_correction: 0.10,
            low_tool_failure_rate: 0.05,
        }
    }
}

/// Deterministic evidence available without user feedback.
#[derive(Debug, Clone, Default)]
pub struct OutcomeEvidence {
    pub terminal_status: Option<AgentRunStatus>,
    pub tools: Vec<ToolUsageSummary>,
    pub has_artifacts: bool,
    pub has_test_success: bool,
    pub immediate_retry: bool,
    pub feedback: Option<RunFeedback>,
}

/// Evaluate an outcome signal from available evidence.
#[must_use]
pub fn evaluate_outcome(evidence: &OutcomeEvidence) -> OutcomeSignal {
    if matches!(
        evidence.terminal_status,
        Some(AgentRunStatus::Cancelled | AgentRunStatus::Interrupted)
    ) {
        return OutcomeSignal::Cancelled;
    }

    let weights = OutcomeWeights::default();
    let mut score = 0.0;

    // Technical completion
    let technical = match evidence.terminal_status {
        Some(AgentRunStatus::Completed) => 1.0,
        Some(AgentRunStatus::Failed) => 0.0,
        _ => 0.4,
    };
    score += weights.technical_completion * technical;

    // Artifact / test success
    let artifact = if evidence.has_test_success {
        1.0
    } else if evidence.has_artifacts {
        0.7
    } else if matches!(evidence.terminal_status, Some(AgentRunStatus::Completed)) {
        0.45
    } else {
        0.1
    };
    score += weights.artifact_or_test_success * artifact;

    // Explicit user feedback (dominant when present)
    if let Some(fb) = &evidence.feedback {
        let fb_score = match fb.rating {
            RunFeedbackRating::Helpful => 1.0,
            RunFeedbackRating::NotHelpful => 0.0,
        };
        score += weights.explicit_user_feedback * fb_score;
    } else {
        // Neutral when absent — do not invent success
        score += weights.explicit_user_feedback * 0.5;
    }

    // No immediate correction / retry
    let no_retry = if evidence.immediate_retry { 0.0 } else { 1.0 };
    score += weights.no_immediate_correction * no_retry;

    // Tool failure rate
    let (calls, fails) = evidence.tools.iter().fold((0u32, 0u32), |acc, t| {
        (acc.0 + t.call_count, acc.1 + t.failure_count)
    });
    let tool_ok = if calls == 0 {
        0.7
    } else {
        1.0 - (f64::from(fails) / f64::from(calls)).min(1.0)
    };
    score += weights.low_tool_failure_rate * tool_ok;

    if score >= 0.75 {
        OutcomeSignal::Successful
    } else if score >= 0.45 {
        if matches!(evidence.terminal_status, Some(AgentRunStatus::Failed)) {
            OutcomeSignal::Failed
        } else {
            OutcomeSignal::PartiallySuccessful
        }
    } else if matches!(evidence.terminal_status, Some(AgentRunStatus::Failed)) {
        OutcomeSignal::Failed
    } else {
        OutcomeSignal::Unknown
    }
}

/// Map a coarse terminal status when no richer evidence is available yet.
#[must_use]
pub fn provisional_outcome(status: AgentRunStatus) -> OutcomeSignal {
    match status {
        AgentRunStatus::Completed => OutcomeSignal::Unknown,
        AgentRunStatus::Failed => OutcomeSignal::Failed,
        AgentRunStatus::Cancelled | AgentRunStatus::Interrupted => OutcomeSignal::Cancelled,
        _ => OutcomeSignal::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn helpful_feedback_promotes_success() {
        let evidence = OutcomeEvidence {
            terminal_status: Some(AgentRunStatus::Completed),
            has_artifacts: true,
            feedback: Some(RunFeedback {
                run_id: app_models::AgentRunId::new(),
                rating: RunFeedbackRating::Helpful,
                comment: None,
                created_at: Utc::now(),
            }),
            ..Default::default()
        };
        assert_eq!(evaluate_outcome(&evidence), OutcomeSignal::Successful);
    }

    #[test]
    fn cancelled_is_not_positive() {
        let evidence = OutcomeEvidence {
            terminal_status: Some(AgentRunStatus::Cancelled),
            ..Default::default()
        };
        assert_eq!(evaluate_outcome(&evidence), OutcomeSignal::Cancelled);
    }
}
