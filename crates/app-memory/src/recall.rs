//! Memory recall engine: relevance-ranked selection for prompt assembly.

use app_models::{
    BehaviorPolicy, MemoryId, MemoryItem, MemoryKind, MemoryRecallHit, MemoryScope, ThreadId,
    WorkflowPattern, WorkflowPatternId, WorkspaceId,
};
use chrono::Utc;

/// Query for recalling memories into a run.
#[derive(Debug, Clone)]
pub struct MemoryRecallQuery {
    pub task: String,
    pub workspace_id: WorkspaceId,
    pub thread_id: ThreadId,
    pub limit: usize,
    pub max_tokens: usize,
}

/// Result of a recall pass including frozen behavior policy.
#[derive(Debug, Clone)]
pub struct MemoryRecallResult {
    pub hits: Vec<MemoryRecallHit>,
    pub excluded: Vec<(MemoryId, String)>,
    pub policy: BehaviorPolicy,
}

/// Rank and select memories for the current task.
///
/// Scoring (first stage — keyword + scope, no vector DB):
/// ```text
/// score =
///   0.35 * relevance
/// + 0.20 * scope_priority
/// + 0.15 * user_confirmation
/// + 0.10 * historical_success
/// + 0.10 * recency
/// + 0.10 * explicitness
/// - conflict_penalty
/// - failure_penalty
/// ```
#[must_use]
pub fn recall_memories(
    query: &MemoryRecallQuery,
    candidates: &[MemoryItem],
    active_patterns: &[WorkflowPattern],
) -> MemoryRecallResult {
    let task_lower = query.task.to_lowercase();
    let task_tokens = tokenize(&task_lower);
    let mut scored: Vec<MemoryRecallHit> = Vec::new();
    let mut excluded = Vec::new();

    for memory in candidates {
        if memory.sensitive {
            excluded.push((memory.id, "sensitive_blocked".into()));
            continue;
        }
        let (score, reasons) = score_memory(memory, &task_lower, &task_tokens);
        if score <= 0.05 {
            excluded.push((memory.id, "low_relevance".into()));
            continue;
        }
        scored.push(MemoryRecallHit {
            memory: memory.clone(),
            score,
            reasons,
        });
    }

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.memory.updated_at.cmp(&a.memory.updated_at))
    });

    // Conflict resolution: higher scope rank (Thread > Workspace > User) wins on same key.
    let mut selected: Vec<MemoryRecallHit> = Vec::new();
    let mut used_keys: std::collections::HashMap<String, MemoryScope> =
        std::collections::HashMap::new();
    for hit in scored {
        let key = hit.memory.key.to_lowercase();
        if let Some(existing_scope) = used_keys.get(&key) {
            if scope_rank(hit.memory.scope) <= scope_rank(*existing_scope) {
                excluded.push((hit.memory.id, format!("conflict_with_key:{key}")));
                continue;
            }
            // Replace lower-priority hit with same key.
            selected.retain(|h| h.memory.key.to_lowercase() != key);
        }
        used_keys.insert(key, hit.memory.scope);
        selected.push(hit);
        if selected.len() >= query.limit {
            break;
        }
    }

    // Token budget trim (rough chars/4).
    let mut token_budget = query.max_tokens.max(64);
    let mut final_hits = Vec::new();
    for hit in selected {
        let cost = estimate_memory_tokens(&hit.memory);
        if cost > token_budget && !final_hits.is_empty() {
            excluded.push((hit.memory.id, "token_budget".into()));
            continue;
        }
        token_budget = token_budget.saturating_sub(cost);
        final_hits.push(hit);
    }

    let policy = synthesize_behavior_policy(&final_hits, active_patterns);
    MemoryRecallResult {
        hits: final_hits,
        excluded,
        policy,
    }
}

/// Build a [`BehaviorPolicy`] from accepted memories + active patterns.
#[must_use]
pub fn synthesize_behavior_policy(
    hits: &[MemoryRecallHit],
    active_patterns: &[WorkflowPattern],
) -> BehaviorPolicy {
    let mut policy = BehaviorPolicy::default();
    for hit in hits {
        policy.memory_ids.push(hit.memory.id);
        apply_memory_to_policy(&mut policy, &hit.memory);
    }
    for pattern in active_patterns {
        policy.pattern_ids.push(pattern.id);
        if policy.response_style.is_none() && !pattern.collaboration_style.is_empty() {
            policy.response_style = Some(pattern.collaboration_style.clone());
        }
        if policy.preferred_output_format.is_none() && !pattern.output_kind.is_empty() {
            policy.preferred_output_format = Some(pattern.output_kind.clone());
        }
        if pattern.tool_strategy.contains("explore") {
            policy.explore_before_edit = true;
        }
        if pattern.tool_strategy.contains("test") {
            policy.run_tests_after_edit = true;
        }
    }
    policy
}

/// Render a BehaviorPolicy as a prompt-safe instruction block.
#[must_use]
pub fn format_behavior_policy_for_prompt(policy: &BehaviorPolicy) -> String {
    let mut lines = Vec::new();
    if let Some(lang) = &policy.response_language {
        lines.push(format!("- Respond in language: {lang}"));
    }
    if let Some(style) = &policy.response_style {
        lines.push(format!("- Response style: {style}"));
    }
    if policy.explore_before_edit {
        lines.push("- Explore (list/search/read) before editing files.".into());
    }
    if policy.run_tests_after_edit {
        lines.push("- Run tests after substantive edits.".into());
        if !policy.preferred_test_commands.is_empty() {
            lines.push(format!(
                "- Preferred test commands: {}",
                policy.preferred_test_commands.join(", ")
            ));
        }
    }
    if let Some(fmt) = &policy.preferred_output_format {
        lines.push(format!("- Preferred output format: {fmt}"));
    }
    for constraint in &policy.negative_constraints {
        lines.push(format!("- Do NOT: {constraint}"));
    }
    if lines.is_empty() {
        return String::new();
    }
    format!("## Learned user preferences (confirmed)\n{}", lines.join("\n"))
}

/// Record usage timestamps (caller persists via MemoryManager).
#[must_use]
pub fn touch_usage_ids(hits: &[MemoryRecallHit]) -> Vec<MemoryId> {
    hits.iter().map(|h| h.memory.id).collect()
}

fn apply_memory_to_policy(policy: &mut BehaviorPolicy, memory: &MemoryItem) {
    let key = memory.key.to_lowercase();
    let value = memory.value.trim();
    if key.contains("response_language") || key == "language" {
        policy.response_language = Some(normalize_language(value));
    } else if key.contains("response_style") || key.contains("style") {
        policy.response_style = Some(value.to_owned());
    } else if key.contains("run_tests") || value.contains("cargo test") || value.contains("测试")
    {
        policy.run_tests_after_edit = true;
        if value.contains("cargo test") {
            policy.preferred_test_commands.push("cargo test".into());
        }
        if value.contains("pnpm test") {
            policy.preferred_test_commands.push("pnpm test".into());
        }
    } else if key.contains("explore") {
        policy.explore_before_edit = true;
    } else if key.contains("output") || key.contains("format") {
        policy.preferred_output_format = Some(value.to_owned());
    } else if matches!(memory.kind, Some(MemoryKind::NegativeConstraint))
        || key.contains("negative")
    {
        policy.negative_constraints.push(value.to_owned());
    } else if value.contains("中文") {
        policy.response_language = Some("zh-CN".into());
    } else if value.to_lowercase().contains("english") {
        policy.response_language = Some("en".into());
    }
}

fn normalize_language(value: &str) -> String {
    let lower = value.to_lowercase();
    if lower.contains("zh") || value.contains("中文") {
        "zh-CN".into()
    } else if lower.starts_with("en") || lower.contains("english") {
        "en".into()
    } else {
        value.to_owned()
    }
}

fn score_memory(memory: &MemoryItem, task_lower: &str, task_tokens: &[String]) -> (f64, Vec<String>) {
    let mut reasons = Vec::new();
    let text = format!("{} {}", memory.key, memory.value).to_lowercase();
    let mem_tokens = tokenize(&text);

    let mut overlap = 0usize;
    for t in task_tokens {
        if t.len() < 2 {
            continue;
        }
        if text.contains(t.as_str()) || mem_tokens.iter().any(|m| m == t) {
            overlap += 1;
        }
    }
    let relevance = if task_tokens.is_empty() {
        0.2
    } else {
        (overlap as f64 / task_tokens.len().max(1) as f64).min(1.0)
    };
    if relevance > 0.0 {
        reasons.push(format!("relevance={relevance:.2}"));
    }

    // Always give mild score to high-confidence explicit preferences.
    let explicitness = if memory.key.contains("response_") || memory.key.contains("preference") {
        0.8
    } else if memory.kind.is_some() {
        0.5
    } else {
        0.3
    };

    let scope_priority = match memory.scope {
        MemoryScope::Thread => 1.0,
        MemoryScope::Workspace => 0.75,
        MemoryScope::User => 0.55,
        MemoryScope::Session => 0.3,
    };

    // Accepted long-term memories are user-confirmed when kind is set from candidates
    // or when they were manually created (default confidence high).
    let user_confirmation = memory.confidence.unwrap_or(0.85).clamp(0.0, 1.0);

    let historical_success = if memory.use_count > 0 {
        (1.0 - (-0.1 * memory.use_count as f64).exp()).min(1.0)
    } else {
        0.3
    };

    let recency = {
        let age = Utc::now()
            .signed_duration_since(memory.updated_at)
            .num_days()
            .max(0) as f64;
        (1.0 / (1.0 + age / 30.0)).clamp(0.1, 1.0)
    };

    let mut score = 0.35 * relevance
        + 0.20 * scope_priority
        + 0.15 * user_confirmation
        + 0.10 * historical_success
        + 0.10 * recency
        + 0.10 * explicitness;

    // Boost exact key matches in task.
    if task_lower.contains(&memory.key.to_lowercase()) {
        score += 0.15;
        reasons.push("key_in_task".into());
    }

    // Mild base for strong user preferences so they surface even without lexical overlap.
    if score < 0.2 && matches!(memory.scope, MemoryScope::User | MemoryScope::Thread) {
        score = 0.22;
        reasons.push("scope_base".into());
    }

    (score, reasons)
}

fn scope_rank(scope: MemoryScope) -> u8 {
    match scope {
        MemoryScope::Thread => 3,
        MemoryScope::Workspace => 2,
        MemoryScope::User => 1,
        MemoryScope::Session => 0,
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_' && !is_cjk(c))
        .filter(|t| t.len() >= 2 || t.chars().any(is_cjk))
        .map(std::string::ToString::to_string)
        .collect()
}

fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
        || ('\u{3400}'..='\u{4dbf}').contains(&c)
        || ('\u{3040}'..='\u{30ff}').contains(&c)
}

fn estimate_memory_tokens(memory: &MemoryItem) -> usize {
    let chars = memory.key.chars().count() + memory.value.chars().count() + 8;
    (chars / 4).max(1)
}

/// Pattern ids from active patterns relevant to the task (for policy snapshot).
#[must_use]
pub fn select_pattern_ids(patterns: &[WorkflowPattern], limit: usize) -> Vec<WorkflowPatternId> {
    patterns.iter().take(limit).map(|p| p.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::MemoryId;
    use chrono::Utc;

    fn mem(scope: MemoryScope, key: &str, value: &str) -> MemoryItem {
        MemoryItem {
            id: MemoryId::new(),
            scope,
            workspace_id: None,
            thread_id: None,
            key: key.into(),
            value: value.into(),
            sensitive: false,
            kind: Some(MemoryKind::UserPreference),
            source_run_id: None,
            confidence: Some(0.9),
            last_used_at: None,
            use_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn thread_overrides_user_on_same_key() {
        let user = mem(MemoryScope::User, "response_language", "en");
        let thread = mem(MemoryScope::Thread, "response_language", "zh-CN");
        let query = MemoryRecallQuery {
            task: "帮我改代码".into(),
            workspace_id: WorkspaceId::new(),
            thread_id: ThreadId::new(),
            limit: 5,
            max_tokens: 500,
        };
        let result = recall_memories(&query, &[user, thread.clone()], &[]);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].memory.id, thread.id);
        assert_eq!(
            result.policy.response_language.as_deref(),
            Some("zh-CN")
        );
    }

    #[test]
    fn sensitive_excluded() {
        let mut m = mem(MemoryScope::User, "secret", "api_key=xxx");
        m.sensitive = true;
        let query = MemoryRecallQuery {
            task: "hello".into(),
            workspace_id: WorkspaceId::new(),
            thread_id: ThreadId::new(),
            limit: 5,
            max_tokens: 500,
        };
        let result = recall_memories(&query, &[m], &[]);
        assert!(result.hits.is_empty());
        assert_eq!(result.excluded.len(), 1);
    }
}
