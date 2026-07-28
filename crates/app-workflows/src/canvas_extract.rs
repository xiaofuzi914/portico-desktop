//! Session relationship + lightweight conversation summaries for the mind map.
//!
//! Product goal: the map is an **overview of project sessions** — title,
//! ≤300-char summary of what was discussed/done, and parent→child branch edges.
//! Legacy narrative branches (intent / progress / conclusion leaves) remain for
//! tests and optional tooling but are no longer the primary project map.

use app_models::{Message, MessageId, MessageRole, Thread, ThreadId};

/// Max threads per project extract.
pub const MAX_THREADS: usize = 48;
/// Scan newest N messages per thread.
pub const MAX_MESSAGES_PER_THREAD: usize = 40;
/// Hard cap on leaf insights per thread (across all branches) for session maps.
pub const MAX_INSIGHTS_PER_THREAD: usize = 8;
/// Max leaves under one narrative branch (session map).
pub const MAX_PER_BRANCH: usize = 3;
/// Project canvas: few summary cards per session so the map stays scannable.
pub const MAX_PROJECT_LEAVES_PER_THREAD: usize = 3;
/// Product mind-map session card body (what / how / discussed).
pub const MAX_SESSION_SUMMARY_CHARS: usize = 300;
/// Context seed injected into a branched child session.
pub const MAX_BRANCH_CONTEXT_CHARS: usize = 1200;

const MAX_TITLE_CHARS: usize = 56;
const MAX_SUMMARY_CHARS: usize = 140;
const MIN_TITLE_CHARS: usize = 2;

/// One session card on the project mind map (title + summary + parent link).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCard {
    pub thread_id: ThreadId,
    pub title: String,
    /// What this session did / discussed — at most [`MAX_SESSION_SUMMARY_CHARS`].
    pub summary: String,
    pub parent_thread_id: Option<ThreadId>,
    pub message_count: usize,
}

/// Build session cards for the project relationship mind map.
#[must_use]
pub fn extract_session_cards(threads: &[(Thread, Vec<Message>)]) -> Vec<SessionCard> {
    let mut sorted: Vec<&(Thread, Vec<Message>)> = threads.iter().collect();
    sorted.sort_by(|a, b| b.0.updated_at.cmp(&a.0.updated_at));

    sorted
        .into_iter()
        .take(MAX_THREADS)
        .map(|(thread, messages)| {
            let title = {
                let trimmed = thread.title.trim();
                if trimmed.is_empty() {
                    "未命名会话".to_owned()
                } else {
                    trimmed.to_owned()
                }
            };
            let summary = summarize_session_messages(messages, MAX_SESSION_SUMMARY_CHARS);
            SessionCard {
                thread_id: thread.id,
                title,
                summary,
                parent_thread_id: thread.parent_thread_id,
                message_count: messages.len(),
            }
        })
        .collect()
}

/// Summarize a conversation into at most `max_chars` characters.
///
/// Prefers: user intents + assistant outcomes, readable Chinese-friendly prose.
#[must_use]
pub fn summarize_session_messages(messages: &[Message], max_chars: usize) -> String {
    let max_chars = max_chars.max(40);
    if messages.is_empty() {
        return "暂无对话内容".to_owned();
    }

    let mut user_bits: Vec<String> = Vec::new();
    let mut assistant_bits: Vec<String> = Vec::new();

    for message in messages.iter().rev() {
        match message.role {
            MessageRole::User => {
                if let Some(bit) = meaningful_snippet(&message.content, 80) {
                    if !user_bits.iter().any(|u| similar_snippet(u, &bit)) {
                        user_bits.push(bit);
                    }
                }
            }
            MessageRole::Assistant => {
                if let Some(bit) = meaningful_snippet(&message.content, 100) {
                    if !assistant_bits.iter().any(|u| similar_snippet(u, &bit)) {
                        assistant_bits.push(bit);
                    }
                }
            }
            MessageRole::System => {}
        }
        if user_bits.len() >= 2 && assistant_bits.len() >= 2 {
            break;
        }
    }

    user_bits.reverse();
    assistant_bits.reverse();

    let mut parts: Vec<String> = Vec::new();
    if !user_bits.is_empty() {
        parts.push(format!("讨论：{}", user_bits.join("；")));
    }
    if !assistant_bits.is_empty() {
        parts.push(format!("进展：{}", assistant_bits.join("；")));
    }

    if parts.is_empty() {
        // Fallback: first non-empty message body.
        for message in messages {
            if let Some(bit) = meaningful_snippet(&message.content, max_chars) {
                return truncate_chars(&bit, max_chars);
            }
        }
        return "暂无对话内容".to_owned();
    }

    let joined = parts.join(" ");
    truncate_chars(&joined, max_chars)
}

/// Seed text for a child session branched from parent context.
#[must_use]
pub fn build_branch_context_seed(
    parent_title: &str,
    summary: &str,
    messages: &[Message],
) -> String {
    build_branch_context_seed_with_focus(parent_title, summary, messages, None)
}

/// Seed a child session; when `focus` is set (划词发散), it is the primary thread.
#[must_use]
pub fn build_branch_context_seed_with_focus(
    parent_title: &str,
    summary: &str,
    messages: &[Message],
    focus: Option<&str>,
) -> String {
    let focus = focus.map(str::trim).filter(|s| !s.is_empty());

    let mut recent: Vec<String> = Vec::new();
    for message in messages.iter().rev().take(6) {
        let role = match message.role {
            MessageRole::User => "用户",
            MessageRole::Assistant => "助手",
            MessageRole::System => continue,
        };
        if let Some(bit) = meaningful_snippet(&message.content, 160) {
            recent.push(format!("- {role}：{bit}"));
        }
        if recent.len() >= 4 {
            break;
        }
    }
    recent.reverse();

    let mut body = if let Some(focus_text) = focus {
        let clipped = truncate_chars(focus_text, 800);
        format!(
            "【从会话「{}」划词发散】\n用户选中的关注点：\n「{}」\n\n父会话摘要：{}\n",
            parent_title.trim(),
            clipped,
            if summary.trim().is_empty() {
                "（暂无摘要）"
            } else {
                summary.trim()
            }
        )
    } else {
        format!(
            "【从会话「{}」发散】\n上下文摘要：{}\n",
            parent_title.trim(),
            if summary.trim().is_empty() {
                "（暂无摘要）"
            } else {
                summary.trim()
            }
        )
    };
    if !recent.is_empty() {
        body.push_str("\n近期对话要点：\n");
        body.push_str(&recent.join("\n"));
        body.push('\n');
    }
    if focus.is_some() {
        body.push_str(
            "\n请围绕用户选中的关注点继续深入分析或展开讨论；可引用父会话上下文，但回答重心放在该关注点上。",
        );
    } else {
        body.push_str("\n你可以基于以上上下文继续深入，或换一个方向发散讨论。");
    }
    truncate_chars(&body, MAX_BRANCH_CONTEXT_CHARS)
}

/// Short child-session title derived from selected text (划词).
#[must_use]
pub fn branch_title_from_focus(focus: &str, parent_title: &str) -> String {
    let cleaned = focus
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let snippet = cleaned.chars().take(36).collect::<String>();
    if snippet.is_empty() {
        let base = parent_title.trim();
        if base.is_empty() {
            return "发散会话".to_owned();
        }
        return format!("{} · 发散", base.chars().take(60).collect::<String>());
    }
    let ellipsis = if cleaned.chars().count() > 36 { "…" } else { "" };
    format!("{snippet}{ellipsis}")
}

fn meaningful_snippet(content: &str, max: usize) -> Option<String> {
    let body = content.trim();
    if body.is_empty() {
        return None;
    }
    // Prefer first meaningful paragraph / line.
    let line = body
        .lines()
        .map(str::trim)
        .find(|l| {
            let n = l.chars().count();
            n >= MIN_TITLE_CHARS && !looks_like_noise(l) && !l.starts_with("【从会话")
        })?;
    let cleaned = clean_title_line(line);
    if cleaned.chars().count() < MIN_TITLE_CHARS || looks_like_noise(&cleaned) {
        return None;
    }
    Some(truncate_chars(&cleaned, max))
}

fn similar_snippet(a: &str, b: &str) -> bool {
    let na = normalize(a);
    let nb = normalize(b);
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    na == nb || na.contains(&nb) || nb.contains(&na)
}

/// Narrative column for a session mind-map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NarrativeBranch {
    /// What the user asked for / intent.
    Intent,
    /// Intermediate decisions, plans, work in progress.
    Progress,
    /// Deliverables, conclusions, outcomes.
    Conclusion,
}

impl NarrativeBranch {
    #[must_use]
    pub const fn label_zh(self) -> &'static str {
        match self {
            Self::Intent => "意图",
            Self::Progress => "推进过程",
            Self::Conclusion => "结论与交付",
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intent => "intent",
            Self::Progress => "progress",
            Self::Conclusion => "conclusion",
        }
    }

    #[must_use]
    pub const fn order(self) -> u8 {
        match self {
            Self::Intent => 0,
            Self::Progress => 1,
            Self::Conclusion => 2,
        }
    }

    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Intent, Self::Progress, Self::Conclusion]
    }
}

/// One extracted leaf linked to a source message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedInsight {
    pub title: String,
    pub summary: String,
    pub thread_id: ThreadId,
    pub message_id: Option<MessageId>,
    pub message_snippet: Option<String>,
    pub branch: NarrativeBranch,
    /// Chronological index within the scanned message window (ascending = earlier).
    pub order_key: i64,
}

/// One narrative branch with leaves (intent / progress / conclusion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedBranch {
    pub branch: NarrativeBranch,
    pub title: String,
    pub insights: Vec<ExtractedInsight>,
}

/// Full session narrative for a single thread (root + up to 3 branches).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionNarrative {
    pub thread_id: ThreadId,
    pub title: String,
    /// One-line human summary for the root card.
    pub summary: String,
    pub branches: Vec<ExtractedBranch>,
}

/// Per-thread cluster (project canvas forest). Insights are **summary** leaves only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedThreadCluster {
    pub thread_id: ThreadId,
    pub title: String,
    /// One-line card subtitle for the session root.
    pub summary: String,
    pub insights: Vec<ExtractedInsight>,
}

/// Extract **project-map** clusters: one card per session + up to 3 summary leaves.
///
/// Prefer intent → progress → conclusion (one each), not a full outline dump.
#[must_use]
pub fn extract_thread_clusters(threads: &[(Thread, Vec<Message>)]) -> Vec<ExtractedThreadCluster> {
    extract_session_narratives(threads)
        .into_iter()
        .map(|n| {
            let insights = summarize_for_project_map(&n);
            let summary = if insights.is_empty() {
                n.summary
            } else {
                project_cluster_summary(&n, &insights)
            };
            ExtractedThreadCluster {
                thread_id: n.thread_id,
                title: n.title,
                summary,
                insights,
            }
        })
        .collect()
}

/// Pick at most one representative leaf per narrative branch for the project map.
fn summarize_for_project_map(narrative: &SessionNarrative) -> Vec<ExtractedInsight> {
    let mut out = Vec::with_capacity(MAX_PROJECT_LEAVES_PER_THREAD);
    for branch in NarrativeBranch::all() {
        let Some(b) = narrative.branches.iter().find(|x| x.branch == branch) else {
            continue;
        };
        if b.insights.is_empty() {
            continue;
        }
        // Intent: earliest; Progress: middle; Conclusion: latest.
        let pick = match branch {
            NarrativeBranch::Intent => b.insights.first(),
            NarrativeBranch::Progress => b
                .insights
                .get(b.insights.len() / 2)
                .or_else(|| b.insights.first()),
            NarrativeBranch::Conclusion => b.insights.last(),
        };
        if let Some(leaf) = pick {
            let mut leaf = leaf.clone();
            // Compact title: keep human text; branch is in payload for styling.
            leaf.title = truncate_chars(&leaf.title, MAX_TITLE_CHARS);
            out.push(leaf);
        }
        if out.len() >= MAX_PROJECT_LEAVES_PER_THREAD {
            break;
        }
    }
    out
}

fn project_cluster_summary(narrative: &SessionNarrative, leaves: &[ExtractedInsight]) -> String {
    let latest = leaves
        .iter()
        .rev()
        .find(|l| l.branch == NarrativeBranch::Conclusion)
        .or_else(|| leaves.last())
        .map(|l| l.title.as_str())
        .unwrap_or("");
    if latest.is_empty() {
        narrative.summary.clone()
    } else {
        format!("{} 要点 · 最近：{}", leaves.len(), truncate_chars(latest, 36))
    }
}

/// Extract structured narratives (session mind-map preferred).
#[must_use]
pub fn extract_session_narratives(threads: &[(Thread, Vec<Message>)]) -> Vec<SessionNarrative> {
    let mut sorted: Vec<&(Thread, Vec<Message>)> = threads.iter().collect();
    sorted.sort_by(|a, b| b.0.updated_at.cmp(&a.0.updated_at));

    sorted
        .into_iter()
        .take(MAX_THREADS)
        .map(|(thread, messages)| build_session_narrative(thread, messages))
        .collect()
}

fn build_session_narrative(thread: &Thread, messages: &[Message]) -> SessionNarrative {
    let title = {
        let trimmed = thread.title.trim();
        if trimmed.is_empty() {
            "Untitled session".to_owned()
        } else {
            trimmed.to_owned()
        }
    };
    let leaves = extract_narrative_leaves(thread.id, messages);
    let branches = group_into_branches(leaves);
    let summary = root_summary(&branches);
    SessionNarrative {
        thread_id: thread.id,
        title,
        summary,
        branches,
    }
}

fn group_into_branches(leaves: Vec<ExtractedInsight>) -> Vec<ExtractedBranch> {
    let mut by_branch: [Vec<ExtractedInsight>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for leaf in leaves {
        let idx = leaf.branch.order() as usize;
        by_branch[idx].push(leaf);
    }
    NarrativeBranch::all()
        .into_iter()
        .filter_map(|branch| {
            let insights = std::mem::take(&mut by_branch[branch.order() as usize]);
            if insights.is_empty() {
                return None;
            }
            Some(ExtractedBranch {
                branch,
                title: branch.label_zh().to_owned(),
                insights,
            })
        })
        .collect()
}

fn root_summary(branches: &[ExtractedBranch]) -> String {
    if branches.is_empty() {
        return "暂无提炼要点 · 对话可能仍在进行".to_owned();
    }
    let total: usize = branches.iter().map(|b| b.insights.len()).sum();
    let mut parts = Vec::new();
    for b in branches {
        parts.push(format!("{} {}", b.insights.len(), b.branch.label_zh()));
    }
    let latest = branches
        .iter()
        .rev()
        .find_map(|b| b.insights.last().map(|i| i.title.as_str()))
        .unwrap_or("");
    if latest.is_empty() {
        format!("{total} 个要点 · {}", parts.join(" · "))
    } else {
        format!("{total} 个要点 · {} · 最近：{latest}", parts.join(" · "))
    }
}

fn extract_narrative_leaves(thread_id: ThreadId, messages: &[Message]) -> Vec<ExtractedInsight> {
    let start = messages.len().saturating_sub(MAX_MESSAGES_PER_THREAD);
    let window = &messages[start..];
    if window.is_empty() {
        return Vec::new();
    }

    let mut candidates: Vec<Candidate> = Vec::new();

    for (idx, message) in window.iter().enumerate() {
        let order_key = idx as i64;
        let progress_ratio = if window.len() <= 1 {
            0.5
        } else {
            idx as f64 / (window.len() - 1) as f64
        };
        match message.role {
            MessageRole::User => {
                if let Some(c) = user_candidate(thread_id, message, order_key, progress_ratio) {
                    candidates.push(c);
                }
            }
            MessageRole::Assistant => {
                candidates.extend(assistant_candidates(
                    thread_id,
                    message,
                    order_key,
                    progress_ratio,
                ));
            }
            MessageRole::System => {}
        }
    }

    let merged = merge_candidates(candidates);
    select_balanced(merged)
}

#[derive(Debug, Clone)]
struct Candidate {
    insight: ExtractedInsight,
    score: i32,
    dedupe_key: String,
}

fn user_candidate(
    thread_id: ThreadId,
    message: &Message,
    order_key: i64,
    progress_ratio: f64,
) -> Option<Candidate> {
    let body = message.content.trim();
    if body.is_empty() {
        return None;
    }
    let first = clean_title_line(body.lines().next()?.trim());
    if first.chars().count() < MIN_TITLE_CHARS || looks_like_noise(&first) {
        return None;
    }
    // Early user turns → intent; later user turns → progress (follow-ups).
    // Single-message window uses ratio 0.5 → still treat as intent.
    let branch = if progress_ratio <= 0.55 {
        NarrativeBranch::Intent
    } else {
        NarrativeBranch::Progress
    };
    let title = humanize_title(&first, branch);
    let summary = truncate_chars(body, MAX_SUMMARY_CHARS);
    let insight = ExtractedInsight {
        title: title.clone(),
        summary: summary.clone(),
        thread_id,
        message_id: Some(message.id),
        message_snippet: Some(summary),
        branch,
        order_key,
    };
    Some(Candidate {
        score: if branch == NarrativeBranch::Intent {
            70
        } else {
            50
        },
        dedupe_key: fuzzy_key(&title),
        insight,
    })
}

fn assistant_candidates(
    thread_id: ThreadId,
    message: &Message,
    order_key: i64,
    progress_ratio: f64,
) -> Vec<Candidate> {
    let body = message.content.trim();
    if body.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();

    for (heading, section) in split_markdown_sections(body) {
        let h = clean_title_line(heading.trim());
        if h.is_empty() || looks_like_noise(&h) {
            continue;
        }
        let (branch, score) = classify_heading(&h, progress_ratio);
        if score < 35 {
            continue;
        }
        // Skip ultra-generic status-only headings when section is empty-ish.
        if is_status_only_heading(&h) && first_meaningful_paragraph(&section).is_none() {
            continue;
        }
        let title = humanize_title(&h, branch);
        let summary = first_meaningful_paragraph(&section)
            .map(|s| truncate_chars(&s, MAX_SUMMARY_CHARS))
            .unwrap_or_else(|| truncate_chars(&title, MAX_SUMMARY_CHARS));
        out.push(Candidate {
            score,
            dedupe_key: fuzzy_key(&title),
            insight: ExtractedInsight {
                title,
                summary: summary.clone(),
                thread_id,
                message_id: Some(message.id),
                message_snippet: Some(summary),
                branch,
                order_key,
            },
        });
    }

    if out.is_empty() {
        if let Some(para) = first_meaningful_paragraph(body) {
            let cleaned = clean_title_line(&para);
            if cleaned.chars().count() >= MIN_TITLE_CHARS && !looks_like_noise(&cleaned) {
                let branch = if progress_ratio > 0.7 {
                    NarrativeBranch::Conclusion
                } else if progress_ratio < 0.35 {
                    NarrativeBranch::Progress
                } else {
                    NarrativeBranch::Progress
                };
                let title = humanize_title(&cleaned, branch);
                let summary = truncate_chars(body, MAX_SUMMARY_CHARS);
                out.push(Candidate {
                    score: 32,
                    dedupe_key: fuzzy_key(&title),
                    insight: ExtractedInsight {
                        title,
                        summary: summary.clone(),
                        thread_id,
                        message_id: Some(message.id),
                        message_snippet: Some(summary),
                        branch,
                        order_key,
                    },
                });
            }
        }
    }

    // Absolute fallback: first non-noise line so the map is never empty for real chat.
    if out.is_empty() {
        let first = body
            .lines()
            .map(str::trim)
            .find(|l| l.chars().count() >= 2 && !looks_like_noise(l));
        if let Some(first) = first {
            let branch = if progress_ratio > 0.65 {
                NarrativeBranch::Conclusion
            } else {
                NarrativeBranch::Progress
            };
            let title = humanize_title(&clean_title_line(first), branch);
            let summary = truncate_chars(body, MAX_SUMMARY_CHARS);
            out.push(Candidate {
                score: 20,
                dedupe_key: fuzzy_key(&title),
                insight: ExtractedInsight {
                    title,
                    summary: summary.clone(),
                    thread_id,
                    message_id: Some(message.id),
                    message_snippet: Some(summary),
                    branch,
                    order_key,
                },
            });
        }
    }

    out
}

fn classify_heading(heading: &str, progress_ratio: f64) -> (NarrativeBranch, i32) {
    let h = heading.to_lowercase();
    let conclusion_keys = [
        "交付",
        "结论",
        "总结",
        "摘要",
        "结果",
        "完成",
        "清单",
        "验收",
        "产出",
        "deliver",
        "summary",
        "conclusion",
        "result",
        "done",
        "ship",
    ];
    let progress_keys = [
        "方案",
        "计划",
        "实现",
        "推进",
        "设计",
        "架构",
        "步骤",
        "改动",
        "修复",
        "验证",
        "分析",
        "plan",
        "design",
        "implement",
        "architecture",
        "fix",
        "next",
        "progress",
    ];
    let intent_keys = [
        "目标",
        "需求",
        "问题",
        "背景",
        "意图",
        "想要",
        "goal",
        "request",
        "problem",
        "background",
    ];

    if conclusion_keys.iter().any(|k| h.contains(k)) {
        return (NarrativeBranch::Conclusion, 90);
    }
    if intent_keys.iter().any(|k| h.contains(k)) {
        return (NarrativeBranch::Intent, 75);
    }
    if progress_keys.iter().any(|k| h.contains(k)) {
        return (NarrativeBranch::Progress, 70);
    }
    // Position bias for generic headings.
    if progress_ratio > 0.7 {
        (NarrativeBranch::Conclusion, 45)
    } else if progress_ratio < 0.3 {
        (NarrativeBranch::Intent, 40)
    } else {
        (NarrativeBranch::Progress, 42)
    }
}

fn is_status_only_heading(h: &str) -> bool {
    let t = clean_title_line(h);
    matches!(
        t.as_str(),
        "完成" | "已完成" | "转换完成" | "交付完成" | "Done" | "OK" | "成功"
    ) || t.chars().all(|c| c == '✓' || c == '✔' || c == '✅' || c.is_whitespace())
}

fn merge_candidates(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    // Prefer higher score, then later messages (richer conclusions) when merging.
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.insight.order_key.cmp(&a.insight.order_key))
    });

    let mut kept: Vec<Candidate> = Vec::new();
    for cand in candidates {
        if let Some(existing) = kept
            .iter_mut()
            .find(|k| similar_keys(&k.dedupe_key, &cand.dedupe_key))
        {
            // Keep higher score; if equal prefer longer summary / later order.
            if cand.score > existing.score
                || (cand.score == existing.score
                    && cand.insight.summary.len() > existing.insight.summary.len())
            {
                // Preserve earlier order_key when merging duplicates of the same idea.
                let order = existing.insight.order_key.min(cand.insight.order_key);
                *existing = cand;
                existing.insight.order_key = order;
            }
            continue;
        }
        kept.push(cand);
    }
    kept
}

fn select_balanced(mut candidates: Vec<Candidate>) -> Vec<ExtractedInsight> {
    // Within each branch keep chronological order, cap per branch.
    let mut buckets: [Vec<Candidate>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for c in candidates.drain(..) {
        buckets[c.insight.branch.order() as usize].push(c);
    }

    let mut selected: Vec<ExtractedInsight> = Vec::new();
    for branch in NarrativeBranch::all() {
        let mut bucket = std::mem::take(&mut buckets[branch.order() as usize]);
        bucket.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.insight.order_key.cmp(&b.insight.order_key))
        });
        bucket.truncate(MAX_PER_BRANCH);
        bucket.sort_by_key(|c| c.insight.order_key);
        for c in bucket {
            selected.push(c.insight);
            if selected.len() >= MAX_INSIGHTS_PER_THREAD {
                return sort_leaves(selected);
            }
        }
    }
    sort_leaves(selected)
}

fn sort_leaves(mut leaves: Vec<ExtractedInsight>) -> Vec<ExtractedInsight> {
    leaves.sort_by(|a, b| {
        a.branch
            .order()
            .cmp(&b.branch.order())
            .then_with(|| a.order_key.cmp(&b.order_key))
    });
    leaves
}

fn humanize_title(raw: &str, branch: NarrativeBranch) -> String {
    let mut t = clean_title_line(raw);
    // Drop redundant branch words when they're the whole title.
    for noise in ["本次", "本轮", "本次的"] {
        if let Some(rest) = t.strip_prefix(noise) {
            t = rest.trim().to_owned();
        }
    }
    // Status-only → branch-aware label
    if is_status_only_heading(&t) {
        t = match branch {
            NarrativeBranch::Conclusion => "交付完成".to_owned(),
            NarrativeBranch::Progress => "推进中".to_owned(),
            NarrativeBranch::Intent => "目标确认".to_owned(),
        };
    }
    if t.chars().count() < MIN_TITLE_CHARS {
        t = branch.label_zh().to_owned();
    }
    truncate_chars(&t, MAX_TITLE_CHARS)
}

fn clean_title_line(s: &str) -> String {
    let mut t = s.trim().to_owned();
    // Strip leading emoji / checkmarks / markdown
    while let Some(c) = t.chars().next() {
        if c == '#' || c == '*' || c == '`' || c == '✓' || c == '✔' || c == '✅' || c == '☐' || c == '☑'
        {
            t = t[c.len_utf8()..].trim_start().to_owned();
            continue;
        }
        break;
    }
    t = t.trim_matches(|c: char| c == '*' || c == '`' || c == '"').trim().to_owned();
    // Collapse whitespace
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fuzzy_key(title: &str) -> String {
    let mut s = normalize(title);
    for prefix in ["本次", "本轮", "关于", "the ", "a "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim().to_owned();
        }
    }
    // Strip common suffix noise
    for suffix in ["完成", "结果", "清单", "摘要"] {
        if s.ends_with(suffix) && s.chars().count() > suffix.chars().count() + 1 {
            // keep the stem for better merging of 交付清单 / 本次交付清单
        }
    }
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn similar_keys(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Containment for near-duplicates: 交付清单 vs 本次交付清单
    if a.contains(b) || b.contains(a) {
        let shorter = a.len().min(b.len());
        let longer = a.len().max(b.len());
        return shorter >= 4 && longer <= shorter * 3;
    }
    false
}

/// Split on markdown headings; returns (heading, body until next heading).
fn split_markdown_sections(body: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current_h = String::new();
    let mut current_b = String::new();

    for line in body.lines() {
        if let Some(h) = strip_heading_only(line) {
            if !current_h.is_empty() || !current_b.trim().is_empty() {
                sections.push((current_h, current_b));
            }
            current_h = h;
            current_b = String::new();
        } else {
            current_b.push_str(line);
            current_b.push('\n');
        }
    }
    if !current_h.is_empty() || !current_b.trim().is_empty() {
        sections.push((current_h, current_b));
    }
    sections
}

/// Only ATX headings (`#`…), not list bullets.
fn strip_heading_only(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=3).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let text = rest.trim();
    if text.is_empty() {
        return None;
    }
    let text = text.trim_matches('*').trim().to_owned();
    Some(text)
}

fn first_meaningful_paragraph(body: &str) -> Option<String> {
    for para in body.split("\n\n") {
        let t = para
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("```") && !l.starts_with('|'))
            .collect::<Vec<_>>()
            .join(" ");
        let t = clean_title_line(&t);
        if t.chars().count() >= MIN_TITLE_CHARS && !looks_like_noise(&t) && !is_status_only_heading(&t)
        {
            return Some(t);
        }
    }
    for line in body.lines() {
        let t = clean_title_line(line.trim());
        if t.chars().count() >= MIN_TITLE_CHARS
            && !looks_like_noise(&t)
            && !t.starts_with("```")
            && !is_status_only_heading(&t)
        {
            return Some(t);
        }
    }
    None
}

fn looks_like_noise(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return true;
    }
    if t.starts_with("```") || t.starts_with("---") {
        return true;
    }
    if t.starts_with("fn ") || t.starts_with("pub ") || t.starts_with("use ") {
        return true;
    }
    if matches!(t.chars().next(), Some('-' | '*' | '•')) {
        return true;
    }
    if t.matches('`').count() >= 4 {
        return true;
    }
    if t.matches('/').count() >= 3 && t.contains('.') {
        return true;
    }
    false
}

fn normalize(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn truncate_chars(value: &str, max: usize) -> String {
    let s: String = value.chars().take(max).collect();
    if value.chars().count() > max {
        format!("{s}…")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::WorkspaceId;

    fn thread(title: &str) -> Thread {
        let now = chrono::Utc::now();
        Thread {
            id: ThreadId::new(),
            workspace_id: WorkspaceId::new(),
            title: title.to_owned(),
            parent_thread_id: None,
            archived_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn message(thread_id: ThreadId, role: MessageRole, content: &str) -> Message {
        Message {
            id: MessageId::new(),
            thread_id,
            run_id: None,
            role,
            content: content.to_owned(),
            client_request_id: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn empty_input_yields_no_clusters() {
        assert!(extract_thread_clusters(&[]).is_empty());
        assert!(extract_session_cards(&[]).is_empty());
    }

    #[test]
    fn session_card_summary_capped_and_branched() {
        let parent = thread("Parent session");
        let mut child = thread("Child session");
        child.parent_thread_id = Some(parent.id);
        let user = message(
            parent.id,
            MessageRole::User,
            "请帮我重构登录流程并补上测试。",
        );
        let assistant = message(
            parent.id,
            MessageRole::Assistant,
            "已完成：抽取 auth 模块，并补充了 3 个单元测试。",
        );
        let cards = extract_session_cards(&[
            (parent.clone(), vec![user, assistant]),
            (child.clone(), vec![]),
        ]);
        assert_eq!(cards.len(), 2);
        let parent_card = cards.iter().find(|c| c.thread_id == parent.id).unwrap();
        assert!(parent_card.summary.chars().count() <= MAX_SESSION_SUMMARY_CHARS);
        assert!(!parent_card.summary.is_empty());
        let child_card = cards.iter().find(|c| c.thread_id == child.id).unwrap();
        assert_eq!(child_card.parent_thread_id, Some(parent.id));
        assert_eq!(child_card.summary, "暂无对话内容");
    }

    #[test]
    fn summarize_session_messages_respects_cap() {
        let long = "字".repeat(500);
        let msg = message(ThreadId::new(), MessageRole::User, &long);
        let s = summarize_session_messages(&[msg], 300);
        assert!(s.chars().count() <= 300);
    }

    #[test]
    fn one_user_message_yields_intent_leaf() {
        let thread = thread("My session");
        let msg = message(thread.id, MessageRole::User, "How do I deploy?\nMore context here.");
        let narratives = extract_session_narratives(&[(thread.clone(), vec![msg])]);
        assert_eq!(narratives.len(), 1);
        assert_eq!(narratives[0].title, "My session");
        assert!(!narratives[0].branches.is_empty());
        let intent = narratives[0]
            .branches
            .iter()
            .find(|b| b.branch == NarrativeBranch::Intent);
        assert!(intent.is_some(), "expected intent branch");
        assert!(intent.unwrap().insights[0].title.contains("deploy") || intent.unwrap().insights[0].title.contains("How"));
    }

    #[test]
    fn blank_thread_title_falls_back() {
        let thread = thread("   ");
        let clusters = extract_thread_clusters(&[(thread, vec![])]);
        assert_eq!(clusters[0].title, "Untitled session");
    }

    #[test]
    fn narrative_groups_delivery_under_conclusion() {
        let thread = thread("架构图");
        let user = message(
            thread.id,
            MessageRole::User,
            "请帮我画一下系统架构图，用 Mermaid。",
        );
        let assistant = message(
            thread.id,
            MessageRole::Assistant,
            "好的，我来梳理。\n\
## 方案设计\n\n采用分层架构。\n\n\
## 本次交付清单\n\n- 图示\n- Mermaid 类型\n\n\
## 交付结果\n\n已输出完整图。\n\n\
## 转换完成\n\n✅\n\n\
## 交付完成\n\n✅",
        );
        let narratives =
            extract_session_narratives(&[(thread, vec![user, assistant])]);
        let n = &narratives[0];
        let branches: Vec<_> = n.branches.iter().map(|b| b.branch).collect();
        assert!(branches.contains(&NarrativeBranch::Intent) || !n.branches.is_empty());
        // Status-only headings should not explode into many near-duplicate leaves.
        let titles: Vec<String> = n
            .branches
            .iter()
            .flat_map(|b| b.insights.iter().map(|i| i.title.clone()))
            .collect();
        assert!(
            titles.len() <= MAX_INSIGHTS_PER_THREAD,
            "too many leaves: {titles:?}"
        );
        // Near-duplicate 交付* should be merged down.
        let deliveryish = titles
            .iter()
            .filter(|t| t.contains("交付") || t.contains("清单"))
            .count();
        assert!(
            deliveryish <= 2,
            "delivery titles not merged: {titles:?}"
        );
        let conclusion = n
            .branches
            .iter()
            .find(|b| b.branch == NarrativeBranch::Conclusion);
        assert!(
            conclusion.is_some(),
            "expected conclusion branch, titles={titles:?}"
        );
    }

    #[test]
    fn does_not_extract_list_bullets_as_primary_insights() {
        let thread = thread("s");
        let msg = message(
            thread.id,
            MessageRole::Assistant,
            "- only bullets\n- more bullets\n- still bullets",
        );
        let clusters = extract_thread_clusters(&[(thread, vec![msg])]);
        assert!(clusters[0].insights.len() <= 1);
    }

    #[test]
    fn duplicate_titles_are_deduped_within_a_thread() {
        let thread = thread("s");
        let first = message(thread.id, MessageRole::User, "Same question");
        let second = message(thread.id, MessageRole::User, "same   question");
        let clusters = extract_thread_clusters(&[(thread, vec![first, second])]);
        assert_eq!(clusters[0].insights.len(), 1);
    }

    #[test]
    fn empty_and_whitespace_messages_are_skipped() {
        let thread = thread("s");
        let messages = vec![
            message(thread.id, MessageRole::User, "   \n  "),
            message(thread.id, MessageRole::Assistant, "\n\n"),
            message(thread.id, MessageRole::System, "ignored"),
        ];
        let clusters = extract_thread_clusters(&[(thread, messages)]);
        assert!(clusters[0].insights.is_empty());
    }

    #[test]
    fn insight_cap_respected() {
        let thread = thread("s");
        let mut messages = vec![message(
            thread.id,
            MessageRole::User,
            "我要做一个完整的架构改造目标说明",
        )];
        for i in 0..12 {
            messages.push(message(
                thread.id,
                MessageRole::Assistant,
                &format!("## 结论要点{i}\n\nbody {i} with enough text for summary"),
            ));
        }
        let narratives = extract_session_narratives(&[(thread.clone(), messages.clone())]);
        let session_leaves: usize = narratives[0].branches.iter().map(|b| b.insights.len()).sum();
        assert!(session_leaves <= MAX_INSIGHTS_PER_THREAD);

        let clusters = extract_thread_clusters(&[(thread, messages)]);
        // Project map is stricter: at most one card per narrative branch.
        assert!(clusters[0].insights.len() <= MAX_PROJECT_LEAVES_PER_THREAD);
    }

    #[test]
    fn project_map_keeps_at_most_three_summary_leaves() {
        let thread = thread("架构图");
        let messages = vec![
            message(thread.id, MessageRole::User, "请帮我画系统架构图"),
            message(
                thread.id,
                MessageRole::Assistant,
                "## 方案设计\n\n分层。\n\n## 实现步骤\n\n画图。\n\n## 交付结果\n\n完成。\n\n## 交付清单\n\n清单A。\n\n## 转换完成\n\n✅",
            ),
        ];
        let clusters = extract_thread_clusters(&[(thread, messages)]);
        assert!(
            clusters[0].insights.len() <= MAX_PROJECT_LEAVES_PER_THREAD,
            "got {}",
            clusters[0].insights.len()
        );
        assert!(!clusters[0].summary.is_empty());
    }

    #[test]
    fn thread_cap_keeps_most_recently_updated() {
        let now = chrono::Utc::now();
        let total = MAX_THREADS + 10;
        let mut pairs: Vec<(Thread, Vec<Message>)> = (0..total)
            .map(|i| {
                let mut t = thread(&format!("t{i}"));
                // Higher index = more recent.
                t.updated_at = now - chrono::Duration::seconds(i64::from((total as i32) - i as i32));
                (t, vec![])
            })
            .collect();
        // Shuffle order so sort-by-updated_at is exercised.
        pairs.reverse();
        let clusters = extract_thread_clusters(&pairs);
        assert_eq!(clusters.len(), MAX_THREADS);
        assert_eq!(clusters[0].title, format!("t{}", total - 1));
        let cards = extract_session_cards(&pairs);
        assert_eq!(cards.len(), MAX_THREADS);
        assert_eq!(cards[0].title, format!("t{}", total - 1));
    }

    #[test]
    fn titles_and_summaries_are_truncated() {
        let thread = thread("s");
        let long = "字".repeat(200);
        let msg = message(thread.id, MessageRole::User, &long);
        let clusters = extract_thread_clusters(&[(thread, vec![msg])]);
        assert!(!clusters[0].insights.is_empty());
        assert!(clusters[0].insights[0].title.chars().count() <= MAX_TITLE_CHARS + 1);
    }

    #[test]
    fn leaves_ordered_by_branch_then_time() {
        let thread = thread("s");
        let messages = vec![
            message(thread.id, MessageRole::User, "目标：画架构图"),
            message(
                thread.id,
                MessageRole::Assistant,
                "## 方案设计\n\n分层组件。\n\n## 交付结果\n\n图已完成。",
            ),
        ];
        let leaves = extract_narrative_leaves(thread.id, &messages);
        for w in leaves.windows(2) {
            let ord = (
                w[0].branch.order(),
                w[0].order_key,
                w[1].branch.order(),
                w[1].order_key,
            );
            assert!(
                (w[0].branch.order(), w[0].order_key) <= (w[1].branch.order(), w[1].order_key),
                "out of order: {ord:?}"
            );
        }
    }
}
