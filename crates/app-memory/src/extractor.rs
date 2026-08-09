//! Local rule-based candidate memory extractor.
//!
//! Automatic extraction only creates Candidates — never long-term memories.

use crate::candidate::candidate_fingerprint;
use app_models::{
    AgentRunId, CandidateStatus, ExperienceEvent, MemoryCandidate, MemoryCandidateId, MemoryKind,
    MemoryScope, OutcomeSignal, ThreadId, WorkspaceId,
};
use chrono::Utc;

/// Extractor version; bump when rule semantics change (allows re-analysis).
pub const EXTRACTOR_VERSION: u32 = 1;

/// A single extraction hit before persistence.
#[derive(Debug, Clone)]
pub struct ExtractedCandidate {
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub sensitive: bool,
    pub evidence: Vec<String>,
}

/// Deterministic local rules that pull stable preferences from task text.
///
/// Patterns (CN/EN):
/// - "以后/始终/请记住/不要再/默认使用/always/remember/never/don't/default to"
#[must_use]
pub fn extract_from_text(task_text: &str) -> Vec<ExtractedCandidate> {
    let mut out = Vec::new();
    let trimmed = task_text.trim();
    if trimmed.is_empty() {
        return out;
    }

    for line in trimmed.lines().chain(std::iter::once(trimmed)) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(c) = match_chinese_rules(line) {
            out.push(c);
            continue;
        }
        if let Some(c) = match_english_rules(line) {
            out.push(c);
        }
    }

    // Whole-message fallback when no line matched but strong phrase is present.
    if out.is_empty() {
        if let Some(c) = match_chinese_rules(trimmed) {
            out.push(c);
        } else if let Some(c) = match_english_rules(trimmed) {
            out.push(c);
        }
    }

    dedupe_extractions(out)
}

/// Extract candidates from a finished experience event.
///
/// Failed / cancelled runs only yield negative constraints ("don't do X") when
/// the user text is explicit; positive preferences require non-failed outcomes.
#[must_use]
pub fn extract_from_experience(event: &ExperienceEvent) -> Vec<MemoryCandidate> {
    let raw = extract_from_text(&event.task_text);
    raw.into_iter()
        .filter(|c| {
            if matches!(c.kind, MemoryKind::NegativeConstraint) {
                return true;
            }
            // Do not promote positive long-term habits from failed runs.
            event.outcome.is_positive_evidence()
                || event.outcome == OutcomeSignal::Unknown
                    && event.terminal_status == app_models::AgentRunStatus::Completed
        })
        .map(|c| to_candidate(c, event.run_id, event.workspace_id, event.thread_id))
        .collect()
}

fn to_candidate(
    extracted: ExtractedCandidate,
    run_id: AgentRunId,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
) -> MemoryCandidate {
    let fingerprint =
        candidate_fingerprint(extracted.scope, extracted.kind, &extracted.key, &extracted.value);
    let workspace_id = match extracted.scope {
        MemoryScope::User | MemoryScope::Session => None,
        MemoryScope::Workspace | MemoryScope::Thread => Some(workspace_id),
    };
    let thread_id = match extracted.scope {
        MemoryScope::Thread => Some(thread_id),
        _ => None,
    };
    MemoryCandidate {
        id: MemoryCandidateId::new(),
        run_id,
        workspace_id,
        thread_id,
        scope: extracted.scope,
        kind: extracted.kind,
        key: extracted.key,
        value: extracted.value,
        fingerprint,
        confidence: extracted.confidence,
        sensitive: extracted.sensitive,
        evidence: extracted.evidence,
        status: CandidateStatus::Proposed,
        extractor_version: EXTRACTOR_VERSION,
        created_at: Utc::now(),
        reviewed_at: None,
    }
}

fn match_chinese_rules(line: &str) -> Option<ExtractedCandidate> {
    let lower = line.to_lowercase();
    // 不要再 / 别再 / 永远不要
    if let Some(rest) = strip_any(line, &["不要再", "别再", "永远不要", "请不要", "不要"]) {
        if rest.chars().count() >= 2 {
            return Some(ExtractedCandidate {
                scope: MemoryScope::User,
                kind: MemoryKind::NegativeConstraint,
                key: "negative_constraint".into(),
                value: rest.trim().to_owned(),
                confidence: 0.85,
                sensitive: false,
                evidence: vec![line.to_owned()],
            });
        }
    }
    // Language preference early (also covered later as whole-line fallback).
    if line.contains("中文")
        && (line.contains("回答") || line.contains("回复") || line.contains("用中文"))
    {
        return Some(ExtractedCandidate {
            scope: MemoryScope::User,
            kind: MemoryKind::UserPreference,
            key: "response_language".into(),
            value: "zh-CN".into(),
            confidence: 0.92,
            sensitive: false,
            evidence: vec![line.to_owned()],
        });
    }
    if (line.contains("英文") || line.contains("英语"))
        && (line.contains("回答") || line.contains("回复"))
    {
        return Some(ExtractedCandidate {
            scope: MemoryScope::User,
            kind: MemoryKind::UserPreference,
            key: "response_language".into(),
            value: "en".into(),
            confidence: 0.9,
            sensitive: false,
            evidence: vec![line.to_owned()],
        });
    }

    // 以后 / 始终 / 请记住 / 请记着
    if let Some(rest) = strip_any(line, &["请记住", "请记着", "记住", "以后都", "以后", "始终", "总是"])
    {
        let rest = rest.trim_start_matches(['：', ':', '，', ',', ' ']).trim();
        if rest.chars().count() >= 2 {
            let (kind, key, scope) = classify_preference(rest);
            let value = if key == "response_language" {
                normalize_language_value(rest)
            } else {
                rest.to_owned()
            };
            return Some(ExtractedCandidate {
                scope,
                kind,
                key,
                value,
                confidence: 0.9,
                sensitive: looks_sensitive(rest),
                evidence: vec![line.to_owned()],
            });
        }
    }
    // 这个项目必须 / 本项目必须
    if let Some(rest) = strip_any(line, &["这个项目必须", "本项目必须", "项目里必须", "项目必须"]) {
        let rest = rest.trim_start_matches(['：', ':', '，', ',', ' ']).trim();
        if rest.chars().count() >= 2 {
            return Some(ExtractedCandidate {
                scope: MemoryScope::Workspace,
                kind: MemoryKind::WorkspaceConvention,
                key: "project_must".into(),
                value: rest.to_owned(),
                confidence: 0.88,
                sensitive: looks_sensitive(rest),
                evidence: vec![line.to_owned()],
            });
        }
    }
    // 默认使用
    if let Some(rest) = strip_any(line, &["默认使用", "默认用"]) {
        let rest = rest.trim_start_matches(['：', ':', '，', ',', ' ']).trim();
        if rest.chars().count() >= 1 {
            return Some(ExtractedCandidate {
                scope: MemoryScope::User,
                kind: MemoryKind::ToolPreference,
                key: "default_tool_or_style".into(),
                value: rest.to_owned(),
                confidence: 0.86,
                sensitive: false,
                evidence: vec![line.to_owned()],
            });
        }
    }
    // Language preference: 用中文 / 中文回答
    if lower.contains("中文")
        && (line.contains("回答") || line.contains("回复") || line.contains("用中文"))
    {
        return Some(ExtractedCandidate {
            scope: MemoryScope::User,
            kind: MemoryKind::UserPreference,
            key: "response_language".into(),
            value: "zh-CN".into(),
            confidence: 0.92,
            sensitive: false,
            evidence: vec![line.to_owned()],
        });
    }
    if (lower.contains("英文") || lower.contains("英语"))
        && (line.contains("回答") || line.contains("回复"))
    {
        return Some(ExtractedCandidate {
            scope: MemoryScope::User,
            kind: MemoryKind::UserPreference,
            key: "response_language".into(),
            value: "en".into(),
            confidence: 0.9,
            sensitive: false,
            evidence: vec![line.to_owned()],
        });
    }
    None
}

fn match_english_rules(line: &str) -> Option<ExtractedCandidate> {
    let lower = line.to_lowercase();
    if let Some(rest) = strip_any_ci(&lower, &["never ", "don't ", "do not ", "stop "]) {
        if rest.len() >= 3 {
            return Some(ExtractedCandidate {
                scope: MemoryScope::User,
                kind: MemoryKind::NegativeConstraint,
                key: "negative_constraint".into(),
                value: rest.trim().to_owned(),
                confidence: 0.82,
                sensitive: false,
                evidence: vec![line.to_owned()],
            });
        }
    }
    if let Some(rest) = strip_any_ci(
        &lower,
        &[
            "always ",
            "please remember ",
            "remember that ",
            "remember ",
            "from now on ",
            "going forward ",
        ],
    ) {
        let rest = rest.trim_start_matches([':', '-', ' ']).trim();
        if rest.len() >= 3 {
            let (kind, key, scope) = classify_preference(rest);
            return Some(ExtractedCandidate {
                scope,
                kind,
                key,
                value: rest.to_owned(),
                confidence: 0.88,
                sensitive: looks_sensitive(rest),
                evidence: vec![line.to_owned()],
            });
        }
    }
    if let Some(rest) = strip_any_ci(&lower, &["default to ", "prefer ", "use "]) {
        if lower.contains("default") || lower.starts_with("prefer ") || lower.starts_with("use ") {
            let rest = rest.trim();
            if rest.len() >= 2 {
                return Some(ExtractedCandidate {
                    scope: MemoryScope::User,
                    kind: MemoryKind::ToolPreference,
                    key: "default_tool_or_style".into(),
                    value: rest.to_owned(),
                    confidence: 0.75,
                    sensitive: false,
                    evidence: vec![line.to_owned()],
                });
            }
        }
    }
    if lower.contains("in chinese") || lower.contains("respond in chinese") || lower.contains("answer in chinese")
    {
        return Some(ExtractedCandidate {
            scope: MemoryScope::User,
            kind: MemoryKind::UserPreference,
            key: "response_language".into(),
            value: "zh-CN".into(),
            confidence: 0.9,
            sensitive: false,
            evidence: vec![line.to_owned()],
        });
    }
    if lower.contains("in english")
        && (lower.contains("respond") || lower.contains("answer") || lower.contains("reply"))
    {
        return Some(ExtractedCandidate {
            scope: MemoryScope::User,
            kind: MemoryKind::UserPreference,
            key: "response_language".into(),
            value: "en".into(),
            confidence: 0.9,
            sensitive: false,
            evidence: vec![line.to_owned()],
        });
    }
    None
}

fn classify_preference(text: &str) -> (MemoryKind, String, MemoryScope) {
    let lower = text.to_lowercase();
    if lower.contains("测试")
        || lower.contains("test")
        || lower.contains("cargo test")
        || lower.contains("pnpm test")
    {
        return (
            MemoryKind::ToolPreference,
            "run_tests_after_edit".into(),
            MemoryScope::Workspace,
        );
    }
    if lower.contains("中文") || lower.contains("english") || lower.contains("language") {
        return (
            MemoryKind::UserPreference,
            "response_language".into(),
            MemoryScope::User,
        );
    }
    if lower.contains("项目") || lower.contains("project") || lower.contains("repo") {
        return (
            MemoryKind::WorkspaceConvention,
            "workspace_convention".into(),
            MemoryScope::Workspace,
        );
    }
    if lower.contains("格式") || lower.contains("markdown") || lower.contains("output") {
        return (
            MemoryKind::DeliveryPreference,
            "preferred_output_format".into(),
            MemoryScope::User,
        );
    }
    (
        MemoryKind::UserPreference,
        "user_preference".into(),
        MemoryScope::User,
    )
}

fn normalize_language_value(text: &str) -> String {
    let lower = text.to_lowercase();
    if text.contains("中文") || lower.contains("zh") {
        "zh-CN".into()
    } else if text.contains("英文") || text.contains("英语") || lower.contains("english") {
        "en".into()
    } else {
        text.to_owned()
    }
}

fn looks_sensitive(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("password")
        || lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("api key")
        || lower.contains("token")
        || text.contains("密码")
        || text.contains("密钥")
}

fn strip_any<'a>(input: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for p in prefixes {
        if let Some(rest) = input.strip_prefix(p) {
            return Some(rest);
        }
        if let Some(idx) = input.find(p) {
            // Allow phrases mid-sentence: "好的，以后用中文"
            if idx > 0 {
                let after = &input[idx + p.len()..];
                if !after.is_empty() {
                    return Some(after);
                }
            }
        }
    }
    None
}

fn strip_any_ci<'a>(lower: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for p in prefixes {
        if let Some(rest) = lower.strip_prefix(p) {
            return Some(rest);
        }
        if let Some(idx) = lower.find(p) {
            if idx > 0 {
                let after = &lower[idx + p.len()..];
                if !after.is_empty() {
                    return Some(after);
                }
            }
        }
    }
    None
}

fn dedupe_extractions(items: Vec<ExtractedCandidate>) -> Vec<ExtractedCandidate> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let fp = candidate_fingerprint(item.scope, item.kind, &item.key, &item.value);
        if seen.insert(fp) {
            out.push(item);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_chinese_language_preference() {
        let hits = extract_from_text("以后都用中文回答我");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].key, "response_language");
        assert_eq!(hits[0].value, "zh-CN");
    }

    #[test]
    fn extracts_negative_constraint() {
        let hits = extract_from_text("不要再擅自改 package.json");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, MemoryKind::NegativeConstraint);
    }

    #[test]
    fn extracts_project_must() {
        let hits = extract_from_text("这个项目必须先跑 cargo test 再提交");
        assert_eq!(hits[0].scope, MemoryScope::Workspace);
        assert_eq!(hits[0].kind, MemoryKind::WorkspaceConvention);
    }

    #[test]
    fn extracts_english_always() {
        let hits = extract_from_text("Always respond in English with bullet points");
        assert!(!hits.is_empty());
    }
}
