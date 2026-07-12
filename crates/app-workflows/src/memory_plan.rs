//! Memory-conditioned plan construction.
//!
//! Combines recalled user patterns with lightweight task signals. This is the
//! "habit prior"; the runtime still enforces tools/permissions.
//!
//! **Result-oriented product rule:** when the user asks for a deliverable
//! (diagrams, files, code changes, plantuml, …), prefer roles that can
//! *produce* the outcome. Pure planner-only runs only when the user clearly
//! wants a plan and nothing else.

use app_models::{
    AgentRunId, AgentRunStatus, BuiltInAgent, OrchestrationPlan, PatternHint, SubagentRun,
    WorkflowPatternId,
};
use chrono::Utc;

use crate::AgentRegistry;

/// Hard cap: more roles ⇒ more latency and more timeout failures on remote models.
const MAX_SUBAGENTS: usize = 2;

/// Build an orchestration plan from pattern hints + task text.
#[must_use]
pub fn build_memory_conditioned_plan(
    parent_run_id: AgentRunId,
    task: &str,
    hints: &[PatternHint],
    registry: &AgentRegistry,
) -> OrchestrationPlan {
    let (roles, rationale, pattern_ids) = select_roles(task, hints);
    let mandate = result_oriented_mandate(task);
    let mut subagents = Vec::with_capacity(roles.len());
    for agent in roles {
        let def = registry.built_in(agent);
        // Short task card — do not paste long habit essays into every subagent prompt.
        subagents.push(SubagentRun {
            id: AgentRunId::new(),
            parent_run_id,
            agent_name: def.name.clone(),
            status: AgentRunStatus::Queued,
            task_description: format!(
                "Task:\n{}\n\nMandate:\n{}\n\nRole ({}): {}\nFocus: {}",
                task.trim(),
                mandate,
                def.name,
                def.description,
                def.system_instructions
            ),
            output_summary: None,
            created_at: Utc::now(),
            completed_at: None,
        });
    }

    OrchestrationPlan {
        parent_run_id,
        subagents,
        pattern_ids,
        planning_rationale: rationale,
    }
}

/// Explicit "plan only, do not execute" intent.
#[must_use]
pub fn plan_only_explicit(task: &str) -> bool {
    let lower = task.to_lowercase();
    contains_any(
        &lower,
        &[
            "仅计划",
            "只要计划",
            "只要方案",
            "不要执行",
            "先别改",
            "不要改代码",
            "不要写文件",
            "plan only",
            "planning only",
            "do not implement",
            "don't implement",
            "no code changes",
        ],
    )
}

/// User wants a concrete deliverable, not just advice.
#[must_use]
pub fn wants_deliverable(task: &str) -> bool {
    let lower = task.to_lowercase();
    contains_any(
        &lower,
        &[
            // diagrams / artifacts
            "plantuml",
            "mermaid",
            "结构图",
            "架构图",
            "时序图",
            "类图",
            "流程图",
            "画一",
            "画出",
            "绘制",
            "diagram",
            "图",
            // produce / write
            "生成",
            "输出",
            "写入",
            "落地",
            "交付",
            "可执行",
            "实现",
            "修改",
            "编写",
            "修复",
            "提交",
            "创建文件",
            "写文件",
            "implement",
            "write",
            "create",
            "generate",
            "produce",
            "fix",
            "patch",
            "commit",
            // explicit closed-loop language
            "闭环",
            "结果导向",
            "直接执行",
            "直接落地",
            "做完",
            "完成任务",
        ],
    )
}

/// Whether orchestration should auto-run a write/exec follow-up after plan-style roles.
#[must_use]
pub fn needs_execution_followup(task: &str, completed_roles: &[String]) -> bool {
    if plan_only_explicit(task) {
        return false;
    }
    if !wants_deliverable(task) {
        return false;
    }
    let has_writer = completed_roles.iter().any(|n| {
        matches!(
            n.as_str(),
            "worker" | "doc-writer" | "tester" | "default"
        )
    });
    !has_writer
}

/// Mandate injected into every subagent task card.
#[must_use]
pub fn result_oriented_mandate(task: &str) -> String {
    if plan_only_explicit(task) {
        return "本任务仅需可执行计划与步骤，不要修改仓库或写文件。".to_owned();
    }
    if wants_deliverable(task) {
        return "结果导向闭环：必须交付用户要的具体产物（代码/文档/PlantUML/文件路径/可粘贴内容）。\
禁止只停在「计划/分步说明」就结束。先读仓库再产出；能写文件就写；最终用中文给出交付清单。"
            .to_owned();
    }
    "以可验证结论闭环：直接回答用户问题，给出路径/证据/结论；避免空泛建议与只写计划。"
        .to_owned()
}

#[allow(clippy::too_many_lines)]
fn select_roles(
    task: &str,
    hints: &[PatternHint],
) -> (Vec<BuiltInAgent>, String, Vec<WorkflowPatternId>) {
    let deliverable = wants_deliverable(task) && !plan_only_explicit(task);
    let plan_only = plan_only_explicit(task);

    // 1) Prefer the strongest recalled pattern's role list when parseable —
    //    but upgrade planner-only habits when the user clearly wants delivery.
    if let Some(best) = hints.first() {
        let parsed: Vec<BuiltInAgent> =
            best.preferred_roles.iter().filter_map(|name| parse_role(name)).collect();
        if !parsed.is_empty() {
            let mut roles = dedupe_roles(parsed);
            if deliverable && roles_are_plan_only(&roles) {
                roles = vec![BuiltInAgent::Explorer, BuiltInAgent::Worker];
            }
            let roles = cap_roles(roles);
            let ids: Vec<WorkflowPatternId> = hints.iter().take(3).map(|h| h.id).collect();
            return (
                roles,
                format!(
                    "按用户习惯「{}」分配角色（最多 {} 个）{}",
                    best.name,
                    MAX_SUBAGENTS,
                    if deliverable {
                        "；结果导向：确保有执行/交付角色"
                    } else {
                        ""
                    }
                ),
                ids,
            );
        }
    }

    // 2) Soft task signals (zh/en) — result-first when deliverable is present.
    let lower = task.to_lowercase();
    let mut roles: Vec<BuiltInAgent> = Vec::new();

    let security = contains_any(
        &lower,
        &["安全", "审计", "漏洞", "security", "audit", "cve", "xss"],
    );
    let explore = contains_any(
        &lower,
        &[
            "目录",
            "结构",
            "有哪些",
            "list",
            "explore",
            "文件",
            "folder",
            "项目",
            "架构",
            "代码库",
            "仓库",
            "workspace",
            "repo",
            "看下",
            "看看",
            "plantuml",
            "结构图",
            "架构图",
        ],
    );
    let plan = contains_any(
        &lower,
        &["计划", "拆解", "方案", "plan", "design", "roadmap"],
    );
    let implement = contains_any(
        &lower,
        &["实现", "修改", "编写", "implement", "write", "code", "fix", "落地", "生成", "输出"],
    );
    let test = contains_any(&lower, &["测试", "test", "spec", "单元测试"]);
    let review = contains_any(&lower, &["review", "评审", "code review"]);
    let research = contains_any(&lower, &["研究", "调研", "research", "search"]);
    let docs = contains_any(&lower, &["文档", "doc", "readme", "说明"]);

    // Explicit role tag in the user message (e.g. 【角色】planner) is a hint,
    // not a hard stop — deliverable still upgrades the cast.
    let forced_planner = contains_any(&lower, &["【角色】planner", "[角色]planner", "角色 planner", "role: planner", "role：planner"]);

    if security {
        roles.push(BuiltInAgent::Explorer);
        roles.push(BuiltInAgent::SecurityReviewer);
    } else if review {
        roles.push(BuiltInAgent::Explorer);
        roles.push(BuiltInAgent::Reviewer);
    } else if deliverable || implement {
        // Result path: read then produce (never planner-only).
        roles.push(BuiltInAgent::Explorer);
        if docs && !implement {
            roles.push(BuiltInAgent::DocWriter);
        } else {
            roles.push(BuiltInAgent::Worker);
        }
    } else if plan_only || (plan && forced_planner && !deliverable) {
        roles.push(BuiltInAgent::Planner);
    } else if plan && !deliverable {
        // "help me plan X" without deliverable language → planner only.
        roles.push(BuiltInAgent::Planner);
    } else if test {
        roles.push(BuiltInAgent::Tester);
    } else if research {
        roles.push(BuiltInAgent::Researcher);
    } else if docs {
        roles.push(BuiltInAgent::DocWriter);
    } else if explore {
        roles.push(BuiltInAgent::Explorer);
    } else {
        roles.push(BuiltInAgent::Default);
    }

    let roles = cap_roles(dedupe_roles(roles));
    let rationale = format!(
        "结果导向编排：{} 个角色（上限 {}）。{}{}",
        roles.len(),
        MAX_SUBAGENTS,
        if hints.is_empty() {
            "无强习惯先验，按任务信号分配。".to_owned()
        } else {
            format!(
                "参考习惯：{}。",
                hints.iter().take(2).map(|h| h.name.as_str()).collect::<Vec<_>>().join("、")
            )
        },
        if deliverable {
            " 用户要求交付物：规划后必须执行/产出。"
        } else if plan_only {
            " 用户明确仅需计划。"
        } else {
            ""
        }
    );
    let ids = hints.iter().take(3).map(|h| h.id).collect();
    (roles, rationale, ids)
}

fn roles_are_plan_only(roles: &[BuiltInAgent]) -> bool {
    !roles.is_empty()
        && roles
            .iter()
            .all(|r| matches!(r, BuiltInAgent::Planner))
}

fn cap_roles(roles: Vec<BuiltInAgent>) -> Vec<BuiltInAgent> {
    roles.into_iter().take(MAX_SUBAGENTS).collect()
}

fn parse_role(name: &str) -> Option<BuiltInAgent> {
    match name.trim().to_lowercase().as_str() {
        "default" => Some(BuiltInAgent::Default),
        "explorer" => Some(BuiltInAgent::Explorer),
        "planner" => Some(BuiltInAgent::Planner),
        "worker" => Some(BuiltInAgent::Worker),
        "reviewer" => Some(BuiltInAgent::Reviewer),
        "security-reviewer" | "security_reviewer" | "securityreviewer" => {
            Some(BuiltInAgent::SecurityReviewer)
        }
        "tester" => Some(BuiltInAgent::Tester),
        "researcher" => Some(BuiltInAgent::Researcher),
        "doc-writer" | "doc_writer" | "docwriter" => Some(BuiltInAgent::DocWriter),
        _ => None,
    }
}

fn dedupe_roles(roles: Vec<BuiltInAgent>) -> Vec<BuiltInAgent> {
    let mut out = Vec::new();
    for role in roles {
        if !out.contains(&role) {
            out.push(role);
        }
    }
    out
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::WorkflowPatternId;

    #[test]
    fn pattern_roles_win_over_keywords() {
        let hints = vec![PatternHint {
            id: WorkflowPatternId::new(),
            name: "my-habit".into(),
            summary: "always explore then review".into(),
            preferred_roles: vec!["explorer".into(), "reviewer".into()],
            collaboration_style: "concise".into(),
            strength: 3.0,
            score: 4.0,
        }];
        let (roles, rationale, ids) = select_roles("随便做点什么", &hints);
        assert_eq!(roles, vec![BuiltInAgent::Explorer, BuiltInAgent::Reviewer]);
        assert!(rationale.contains("my-habit"));
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn chinese_security_task_selects_explorer_and_security() {
        let (roles, _, _) = select_roles("请对项目做安全审计", &[]);
        assert!(roles.contains(&BuiltInAgent::Explorer));
        assert!(roles.contains(&BuiltInAgent::SecurityReviewer));
    }

    #[test]
    fn plantuml_architecture_task_selects_explorer_and_worker_not_planner_only() {
        let task = "【任务】沿着项目代码，画一套完整的代码结构图，以PlantUML的形式【角色】planner【要求】给出可执行的分步计划";
        let (roles, rationale, _) = select_roles(task, &[]);
        assert!(
            roles.contains(&BuiltInAgent::Worker) || roles.contains(&BuiltInAgent::DocWriter),
            "deliverable task must include a producer role, got {roles:?}"
        );
        assert!(
            !roles_are_plan_only(&roles),
            "must not be planner-only for PlantUML deliverable"
        );
        assert!(rationale.contains("结果导向") || rationale.contains("交付"));
    }

    #[test]
    fn plan_only_explicit_keeps_planner() {
        let (roles, _, _) = select_roles("请只出方案，仅计划，不要执行，不要写文件", &[]);
        assert_eq!(roles, vec![BuiltInAgent::Planner]);
    }

    #[test]
    fn needs_execution_followup_when_planner_finished_but_deliverable() {
        assert!(needs_execution_followup(
            "生成 PlantUML 结构图并写入 docs/",
            &["planner".into()]
        ));
        assert!(!needs_execution_followup(
            "生成 PlantUML 结构图",
            &["explorer".into(), "worker".into()]
        ));
        assert!(!needs_execution_followup(
            "仅计划，不要执行",
            &["planner".into()]
        ));
    }

    #[test]
    fn habit_planner_only_upgraded_for_deliverable() {
        let hints = vec![PatternHint {
            id: WorkflowPatternId::new(),
            name: "always-plan".into(),
            summary: "plan first".into(),
            preferred_roles: vec!["planner".into()],
            collaboration_style: "brief".into(),
            strength: 2.0,
            score: 3.0,
        }];
        let (roles, _, _) = select_roles("画 PlantUML 架构图并落地到仓库", &hints);
        assert!(roles.contains(&BuiltInAgent::Worker));
    }
}
