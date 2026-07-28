//! Durable multi-stage orchestration graphs (single / foreach / reduce / loop).
//!
//! Pure planning, validation, loop stop, and handoff helpers are unit-tested
//! without I/O. Execution wiring lives in [`crate::orchestration_service`].

use app_models::{
    AgentRunId, OrchestrationPlan, OrchestrationStage, OrchestrationStageKind,
    OrchestrationStageStatus, OrchestrationStageTask, PatternHint, WorkflowPatternId,
};
use serde_json::{json, Value};

/// Catalog entry for a named multi-stage workflow (bundled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledWorkflowMeta {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
}

/// Shipped multi-stage workflows discoverable from the product UI (≥3).
pub const BUNDLED_WORKFLOWS: &[BundledWorkflowMeta] = &[
    BundledWorkflowMeta {
        id: "multi-lens-review",
        title: "Multi-lens review",
        summary: "Plan review lenses → fan-out parallel reviews → synthesize findings",
    },
    BundledWorkflowMeta {
        id: "deep-research",
        title: "Deep research",
        summary: "Plan research questions → fan-out investigations → reduce report",
    },
    BundledWorkflowMeta {
        id: "iterative-refine",
        title: "Iterative refine",
        summary: "Draft → bounded critique/fix loop → final polish",
    },
];

/// List bundled workflow metadata for UI.
#[must_use]
pub fn list_bundled_workflows() -> Vec<BundledWorkflowMeta> {
    BUNDLED_WORKFLOWS.to_vec()
}

fn base_stage(
    id: &str,
    kind: OrchestrationStageKind,
    title: &str,
    agent: &str,
    prompt: String,
    depends_on: Vec<String>,
) -> OrchestrationStage {
    OrchestrationStage {
        id: id.to_owned(),
        kind,
        title: title.to_owned(),
        agent_name: agent.to_owned(),
        status: OrchestrationStageStatus::Pending,
        prompt_template: prompt,
        depends_on,
        foreach_path: None,
        body_stage_ids: vec![],
        max_iterations: None,
        stop_flag_path: None,
        current_iteration: None,
        tasks: vec![],
        output_payload: None,
        error_message: None,
    }
}

/// Build a stage graph plan for a named bundled workflow, or `None` if unknown.
#[must_use]
pub fn plan_bundled_workflow(
    workflow_id: &str,
    parent_run_id: AgentRunId,
    task: &str,
) -> Option<OrchestrationPlan> {
    match workflow_id {
        "multi-lens-review" => Some(plan_multi_lens_review(parent_run_id, task)),
        "deep-research" => Some(plan_deep_research(parent_run_id, task)),
        "iterative-refine" => Some(plan_iterative_refine(parent_run_id, task)),
        _ => None,
    }
}

/// Adaptive multi-stage plan: control plan → role fan-out → reduce synthesis.
#[must_use]
pub fn plan_adaptive_stage_graph(
    parent_run_id: AgentRunId,
    task: &str,
    hints: &[PatternHint],
    preferred_roles: &[String],
) -> OrchestrationPlan {
    let roles: Vec<String> = if preferred_roles.is_empty() {
        default_roles_for_task(task)
    } else {
        preferred_roles.to_vec()
    };
    let roles = if roles.is_empty() {
        vec!["explorer".to_owned(), "reviewer".to_owned()]
    } else {
        roles
    };

    let pattern_ids: Vec<WorkflowPatternId> = hints.iter().map(|h| h.id).collect();
    let rationale = if hints.is_empty() {
        format!(
            "Adaptive multi-stage graph: plan → foreach {} roles → reduce.",
            roles.len()
        )
    } else {
        format!(
            "Adaptive multi-stage graph conditioned by {} pattern(s): plan → foreach → reduce.",
            hints.len()
        )
    };

    let control_items: Vec<Value> = roles
        .iter()
        .enumerate()
        .map(|(i, role)| {
            json!({
                "id": format!("role-{i}"),
                "label": role,
                "agent": role,
                "focus": format!("Apply the {role} lens to: {}", task.trim()),
            })
        })
        .collect();
    let control_s = serde_json::to_string(&json!({ "items": control_items }))
        .unwrap_or_else(|_| "{\"items\":[]}".to_owned());

    let mut plan_stage = base_stage(
        "plan",
        OrchestrationStageKind::Single,
        "Plan work packages",
        "planner",
        format!(
            "Task:\n{task}\n\nProduce a short plan. Prefer machine-readable control JSON:\n\
```json\n{{\"items\":[...]}}\n```\nDefault items if unsure: {control_s}",
            task = task.trim(),
        ),
        vec![],
    );
    plan_stage.output_payload = Some(control_s);

    let mut inspect = base_stage(
        "inspect",
        OrchestrationStageKind::Foreach,
        "Parallel role inspections",
        "reviewer",
        "Task:\n{task}\n\nItem:\n{item}\n\nUpstream plan:\n{upstream}\n\n\
Produce concrete findings for this lens."
            .to_owned(),
        vec!["plan".to_owned()],
    );
    inspect.foreach_path = Some("items".to_owned());

    let report = base_stage(
        "report",
        OrchestrationStageKind::Reduce,
        "Synthesize report",
        "doc-writer",
        "Task:\n{task}\n\nUpstream stage outputs:\n{upstream}\n\n\
Write a result-first synthesis: conclusions, evidence, and next actions."
            .to_owned(),
        vec!["inspect".to_owned()],
    );

    OrchestrationPlan {
        parent_run_id,
        subagents: vec![],
        pattern_ids,
        planning_rationale: rationale,
        stages: vec![plan_stage, inspect, report],
        workflow_id: Some("adaptive".to_owned()),
        workflow_title: Some("Adaptive multi-stage".to_owned()),
    }
}

fn plan_multi_lens_review(parent_run_id: AgentRunId, task: &str) -> OrchestrationPlan {
    let lenses = vec![
        json!({"id":"correctness","label":"Correctness","agent":"reviewer","focus":"logic bugs, edge cases"}),
        json!({"id":"security","label":"Security","agent":"security-reviewer","focus":"auth, injection, secrets"}),
        json!({"id":"tests","label":"Tests","agent":"tester","focus":"coverage gaps, flaky risks"}),
    ];
    let control_s = serde_json::to_string(&json!({ "items": lenses }))
        .unwrap_or_else(|_| "{\"items\":[]}".to_owned());

    let mut triage = base_stage(
        "triage",
        OrchestrationStageKind::Single,
        "Triage review scope",
        "planner",
        format!(
            "Task:\n{}\n\nConfirm or refine review lenses. Emit control JSON with items[]. Default:\n```json\n{control_s}\n```",
            task.trim()
        ),
        vec![],
    );
    triage.output_payload = Some(control_s);

    let mut lenses_stage = base_stage(
        "lenses",
        OrchestrationStageKind::Foreach,
        "Parallel lens reviews",
        "reviewer",
        "Review task:\n{task}\n\nLens:\n{item}\n\nTriage:\n{upstream}\n\nList evidence-backed findings only."
            .to_owned(),
        vec!["triage".to_owned()],
    );
    lenses_stage.foreach_path = Some("items".to_owned());

    let synthesize = base_stage(
        "synthesize",
        OrchestrationStageKind::Reduce,
        "Deduplicate & report",
        "doc-writer",
        "Original task:\n{task}\n\nLens findings:\n{upstream}\n\nFinal review report: summary, must-fix, residual risks."
            .to_owned(),
        vec!["lenses".to_owned()],
    );

    OrchestrationPlan {
        parent_run_id,
        subagents: vec![],
        pattern_ids: vec![],
        planning_rationale:
            "Bundled multi-lens-review: single plan → foreach lenses → reduce findings."
                .to_owned(),
        stages: vec![triage, lenses_stage, synthesize],
        workflow_id: Some("multi-lens-review".to_owned()),
        workflow_title: Some("Multi-lens review".to_owned()),
    }
}

fn plan_deep_research(parent_run_id: AgentRunId, task: &str) -> OrchestrationPlan {
    let questions = vec![
        json!({"id":"q1","label":"Scope & architecture","agent":"explorer","focus":"map structure and entry points"}),
        json!({"id":"q2","label":"Risks & unknowns","agent":"researcher","focus":"open questions and dependencies"}),
        json!({"id":"q3","label":"Evidence & sources","agent":"researcher","focus":"cite concrete paths and facts"}),
    ];
    let control_s = serde_json::to_string(&json!({ "items": questions }))
        .unwrap_or_else(|_| "{\"items\":[]}".to_owned());

    let mut plan = base_stage(
        "plan",
        OrchestrationStageKind::Single,
        "Plan research questions",
        "planner",
        format!(
            "Task:\n{}\n\nBreak into research questions as control JSON items[]. Default:\n```json\n{control_s}\n```",
            task.trim()
        ),
        vec![],
    );
    plan.output_payload = Some(control_s);

    let mut fanout = base_stage(
        "investigate",
        OrchestrationStageKind::Foreach,
        "Investigate questions",
        "researcher",
        "Research task:\n{task}\n\nQuestion:\n{item}\n\nPlan context:\n{upstream}\n\n\
Ground answers in concrete evidence (paths, APIs, behaviors)."
            .to_owned(),
        vec!["plan".to_owned()],
    );
    fanout.foreach_path = Some("items".to_owned());

    let report = base_stage(
        "report",
        OrchestrationStageKind::Reduce,
        "Research report",
        "doc-writer",
        "Task:\n{task}\n\nInvestigation notes:\n{upstream}\n\n\
Write an executive research report: findings, confidence, open questions, next steps."
            .to_owned(),
        vec!["investigate".to_owned()],
    );

    OrchestrationPlan {
        parent_run_id,
        subagents: vec![],
        pattern_ids: vec![],
        planning_rationale: "Bundled deep-research: plan → foreach questions → reduce report."
            .to_owned(),
        stages: vec![plan, fanout, report],
        workflow_id: Some("deep-research".to_owned()),
        workflow_title: Some("Deep research".to_owned()),
    }
}

/// Draft → loop(critique) → polish. Loop body is `critique` only; stop when `pass` is true.
fn plan_iterative_refine(parent_run_id: AgentRunId, task: &str) -> OrchestrationPlan {
    let draft = base_stage(
        "draft",
        OrchestrationStageKind::Single,
        "Initial draft",
        "worker",
        format!(
            "Task:\n{}\n\nProduce a concrete first draft of the deliverable. Prefer files/paths when relevant.",
            task.trim()
        ),
        vec![],
    );

    let critique = base_stage(
        "critique",
        OrchestrationStageKind::Single,
        "Critique & fix",
        "reviewer",
        "Task:\n{task}\n\nCurrent draft / prior output:\n{upstream}\n\n\
Improve the draft. When quality is acceptable, end with control JSON:\n\
```json\n{{\"pass\": true, \"summary\": \"...\"}}\n```\n\
Otherwise: {{\"pass\": false, \"summary\": \"what still fails\"}}."
            .to_owned(),
        // Body stages still list deps for display; loop owns scheduling.
        vec!["draft".to_owned()],
    );

    let mut refine_loop = base_stage(
        "refine_loop",
        OrchestrationStageKind::Loop,
        "Bounded refine loop",
        "planner",
        "Loop container".to_owned(),
        vec!["draft".to_owned()],
    );
    refine_loop.body_stage_ids = vec!["critique".to_owned()];
    refine_loop.max_iterations = Some(3);
    refine_loop.stop_flag_path = Some("pass".to_owned());

    let polish = base_stage(
        "polish",
        OrchestrationStageKind::Reduce,
        "Final polish",
        "doc-writer",
        "Task:\n{task}\n\nDraft and critique history:\n{upstream}\n\n\
Produce the final polished deliverable / report."
            .to_owned(),
        vec!["refine_loop".to_owned()],
    );

    OrchestrationPlan {
        parent_run_id,
        subagents: vec![],
        pattern_ids: vec![],
        planning_rationale:
            "Bundled iterative-refine: draft → loop(critique, max 3, stop on pass) → polish."
                .to_owned(),
        stages: vec![draft, critique, refine_loop, polish],
        workflow_id: Some("iterative-refine".to_owned()),
        workflow_title: Some("Iterative refine".to_owned()),
    }
}

fn default_roles_for_task(task: &str) -> Vec<String> {
    let lower = task.to_lowercase();
    let mut roles = vec!["explorer".to_owned()];
    if lower.contains("security") || lower.contains("安全") || lower.contains("auth") {
        roles.push("security-reviewer".to_owned());
    }
    if lower.contains("test") || lower.contains("测试") {
        roles.push("tester".to_owned());
    }
    if lower.contains("review") || lower.contains("审查") || lower.contains("审计") {
        roles.push("reviewer".to_owned());
    }
    if roles.len() == 1 {
        roles.push("reviewer".to_owned());
    }
    roles
}

/// Topological order of stages by `depends_on`. Loop body stages that are only
/// referenced via `body_stage_ids` are ordered relative to the loop container.
#[must_use]
pub fn topological_stage_order(stages: &[OrchestrationStage]) -> Vec<String> {
    use std::collections::{HashMap, HashSet, VecDeque};
    let ids: HashSet<String> = stages.iter().map(|s| s.id.clone()).collect();
    let mut indeg: HashMap<String, usize> = stages.iter().map(|s| (s.id.clone(), 0)).collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

    // Edges from depends_on, excluding edges into loop-body stages from outside
    // the loop (body is scheduled by the loop). Body stages still depend on each other.
    let body_members: HashSet<String> = stages
        .iter()
        .filter(|s| s.kind == OrchestrationStageKind::Loop)
        .flat_map(|s| s.body_stage_ids.iter().cloned())
        .collect();

    for s in stages {
        for dep in &s.depends_on {
            if !ids.contains(dep) {
                continue;
            }
            // If this stage is a loop body member, only honor deps that are also body members
            // or the loop container is separate — body stages keep internal deps.
            if body_members.contains(&s.id) && !body_members.contains(dep) {
                // Body stages may depend on pre-loop stages conceptually; for topo of
                // top-level schedule we skip body stages entirely.
                continue;
            }
            if body_members.contains(&s.id) {
                continue; // body scheduled by loop
            }
            adj.entry(dep.clone()).or_default().push(s.id.clone());
            *indeg.entry(s.id.clone()).or_default() += 1;
        }
    }

    // Remove body stages from top-level schedule (executed inside loop).
    for b in &body_members {
        indeg.remove(b);
    }

    let mut q: VecDeque<String> = indeg
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| id.clone())
        .collect();
    q.make_contiguous().sort_by_key(|id| {
        stages.iter().position(|s| &s.id == id).unwrap_or(usize::MAX)
    });
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    while let Some(id) = q.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        out.push(id.clone());
        if let Some(nexts) = adj.get(&id) {
            for n in nexts {
                if body_members.contains(n) {
                    continue;
                }
                if let Some(d) = indeg.get_mut(n) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        q.push_back(n.clone());
                    }
                }
            }
        }
    }
    for s in stages {
        if body_members.contains(&s.id) {
            continue;
        }
        if !seen.contains(&s.id) {
            out.push(s.id.clone());
        }
    }
    out
}

/// Extract a JSON array of items from an upstream control payload for foreach expansion.
#[must_use]
pub fn extract_foreach_items(upstream_payload: &str, path: Option<&str>) -> Vec<Value> {
    let Ok(v) = serde_json::from_str::<Value>(upstream_payload) else {
        return fallback_items_from_text(upstream_payload);
    };
    if let Some(items) = find_items_array(&v, path.unwrap_or("items")) {
        return items;
    }
    fallback_items_from_text(upstream_payload)
}

fn find_items_array(v: &Value, key: &str) -> Option<Vec<Value>> {
    if let Some(Value::Array(items)) = v.get(key) {
        if !items.is_empty() {
            return Some(items.clone());
        }
    }
    if key != "items" {
        if let Some(Value::Array(items)) = v.get("items") {
            if !items.is_empty() {
                return Some(items.clone());
            }
        }
    }
    if let Some(Value::Array(items)) = v.pointer("/control/items") {
        if !items.is_empty() {
            return Some(items.clone());
        }
    }
    if let Some(obj) = v.as_object() {
        for val in obj.values() {
            if let Some(items) = find_items_array(val, key) {
                return Some(items);
            }
        }
    }
    if let Some(items) = v.as_array() {
        if !items.is_empty() {
            return Some(items.clone());
        }
    }
    None
}

fn fallback_items_from_text(text: &str) -> Vec<Value> {
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                let slice = &text[start..=end];
                if let Ok(v) = serde_json::from_str::<Value>(slice) {
                    if let Some(items) = find_items_array(&v, "items") {
                        return items;
                    }
                }
            }
        }
    }
    vec![json!({
        "id": "item-0",
        "label": "default",
        "focus": truncate(text, 400),
    })]
}

/// Control JSON for foreach expansion from dependency stages (raw dep payload).
#[must_use]
pub fn primary_upstream_control_payload(
    stages: &[OrchestrationStage],
    depends_on: &[String],
) -> String {
    for dep in depends_on {
        let Some(stage) = stages.iter().find(|s| &s.id == dep) else {
            continue;
        };
        if let Some(payload) = &stage.output_payload {
            let items =
                extract_foreach_items(payload, stage.foreach_path.as_deref().or(Some("items")));
            if items.len() > 1
                || items
                    .first()
                    .and_then(|i| i.get("id"))
                    .and_then(|id| id.as_str())
                    != Some("item-0")
            {
                return payload.clone();
            }
            if payload.contains("\"items\"") {
                return payload.clone();
            }
        }
    }
    depends_on
        .first()
        .and_then(|id| stages.iter().find(|s| &s.id == id))
        .and_then(|s| s.output_payload.clone())
        .unwrap_or_else(|| "{\"items\":[]}".to_owned())
}

/// Evaluate loop stop from the last body stage output / loop control payload.
///
/// Returns true when the loop should **stop** (pass/done flag true).
#[must_use]
pub fn loop_should_stop(control_payload: &str, stop_flag_path: Option<&str>) -> bool {
    let path = stop_flag_path.unwrap_or("pass");
    let Ok(v) = serde_json::from_str::<Value>(control_payload) else {
        // Try extract JSON object from free text.
        if let Some(start) = control_payload.find('{') {
            if let Some(end) = control_payload.rfind('}') {
                if end > start {
                    if let Ok(v) = serde_json::from_str::<Value>(&control_payload[start..=end]) {
                        return flag_is_true(&v, path);
                    }
                }
            }
        }
        return false;
    };
    flag_is_true(&v, path)
}

fn flag_is_true(v: &Value, path: &str) -> bool {
    if let Some(b) = v.get(path).and_then(|x| x.as_bool()) {
        return b;
    }
    // Nested search for common keys
    if let Some(obj) = v.as_object() {
        for (k, val) in obj {
            if k == path {
                if let Some(b) = val.as_bool() {
                    return b;
                }
            }
            if let Some(b) = val.get(path).and_then(|x| x.as_bool()) {
                return b;
            }
        }
    }
    false
}

/// Hard-cap helper: clamp max iterations to a safe range (1..=32).
#[must_use]
pub fn clamp_loop_max_iterations(max: Option<u32>) -> u32 {
    max.unwrap_or(3).clamp(1, 32)
}

/// Expand a foreach stage into pending tasks using upstream payload.
#[must_use]
pub fn expand_foreach_tasks(
    stage: &OrchestrationStage,
    upstream_payload: &str,
) -> Vec<OrchestrationStageTask> {
    let items = extract_foreach_items(upstream_payload, stage.foreach_path.as_deref());
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let label = item
                .get("label")
                .or_else(|| item.get("id"))
                .or_else(|| item.get("name"))
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("item-{i}"));
            OrchestrationStageTask {
                id: format!("{}-task-{i}", stage.id),
                item_index: Some(i as u32),
                label,
                status: OrchestrationStageStatus::Pending,
                subagent_id: None,
                output_summary: None,
                output_payload: Some(item.to_string()),
            }
        })
        .collect()
}

/// Assemble reduce-stage upstream text from completed dependency stage payloads/tasks.
#[must_use]
pub fn assemble_reduce_upstream(stages: &[OrchestrationStage], depends_on: &[String]) -> String {
    let mut parts = Vec::new();
    for dep_id in depends_on {
        let Some(stage) = stages.iter().find(|s| &s.id == dep_id) else {
            continue;
        };
        parts.push(format!("## Stage `{}` — {}", stage.id, stage.title));
        if let Some(iter) = stage.current_iteration {
            parts.push(format!("(loop iteration {iter})"));
        }
        if let Some(payload) = &stage.output_payload {
            parts.push(format!("Control/output JSON:\n{payload}"));
        }
        for task in &stage.tasks {
            let summary = task.output_summary.as_deref().unwrap_or("(no summary)");
            parts.push(format!("### {}\n{summary}", task.label));
        }
    }
    if parts.is_empty() {
        "(no upstream outputs)".to_owned()
    } else {
        parts.join("\n\n")
    }
}

/// Render a stage prompt template with task / upstream / item substitutions.
#[must_use]
pub fn render_stage_prompt(template: &str, task: &str, upstream: &str, item: &str) -> String {
    template
        .replace("{task}", task)
        .replace("{upstream}", upstream)
        .replace("{item}", item)
}

/// Merge foreach task outputs into a single control payload for downstream reduce.
#[must_use]
pub fn merge_foreach_outputs(tasks: &[OrchestrationStageTask]) -> String {
    let items: Vec<Value> = tasks
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "label": t.label,
                "status": t.status.as_str(),
                "summary": t.output_summary,
                "payload": t.output_payload.as_ref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "items": items }))
        .unwrap_or_else(|_| "{\"items\":[]}".to_owned())
}

/// Collect dependency stage payloads into one JSON object keyed by stage id.
#[must_use]
pub fn collect_upstream_payloads(stages: &[OrchestrationStage], depends_on: &[String]) -> String {
    let mut map = serde_json::Map::new();
    for dep in depends_on {
        if let Some(stage) = stages.iter().find(|s| &s.id == dep) {
            if let Some(p) = &stage.output_payload {
                if let Ok(v) = serde_json::from_str::<Value>(p) {
                    map.insert(dep.clone(), v);
                    continue;
                }
            }
            map.insert(dep.clone(), json!({ "summary": stage.title }));
        }
    }
    Value::Object(map).to_string()
}

fn truncate(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        format!("{t}…")
    } else {
        t
    }
}

/// Whether a plan uses the multi-stage graph path.
#[must_use]
pub fn plan_has_stages(plan: &OrchestrationPlan) -> bool {
    !plan.stages.is_empty()
}

/// Validate a stage DAG for save/start. Returns Ok(()) or an error message.
///
/// Free cycles are rejected. Loop body stages may only form the loop's internal
/// schedule (referenced via body_stage_ids), not arbitrary cycles across the graph.
#[must_use]
pub fn validate_stage_dag(stages: &[OrchestrationStage]) -> Result<(), String> {
    if stages.is_empty() {
        return Err("workflow must contain at least one stage".to_owned());
    }
    let mut ids = std::collections::HashSet::new();
    for s in stages {
        if s.id.trim().is_empty() {
            return Err("stage id must not be empty".to_owned());
        }
        if !ids.insert(s.id.clone()) {
            return Err(format!("duplicate stage id: {}", s.id));
        }
        if s.kind == OrchestrationStageKind::Loop {
            let max = clamp_loop_max_iterations(s.max_iterations);
            if max < 1 {
                return Err(format!("loop `{}` max_iterations must be >= 1", s.id));
            }
            if s.body_stage_ids.is_empty() {
                return Err(format!("loop `{}` must list body_stage_ids", s.id));
            }
            for b in &s.body_stage_ids {
                if !stages.iter().any(|x| &x.id == b) {
                    return Err(format!("loop `{}` body stage `{b}` not found", s.id));
                }
                if b == &s.id {
                    return Err(format!("loop `{}` cannot include itself in body", s.id));
                }
            }
        }
        for d in &s.depends_on {
            if !stages.iter().any(|x| &x.id == d) {
                return Err(format!("stage `{}` depends on missing `{d}`", s.id));
            }
        }
    }

    // Cycle detection on depends_on ignoring loop-body-only edges into body members
    // from non-body (those are skipped in topo). Detect cycles among top-level schedule.
    let order = topological_stage_order(stages);
    let body: std::collections::HashSet<_> = stages
        .iter()
        .filter(|s| s.kind == OrchestrationStageKind::Loop)
        .flat_map(|s| s.body_stage_ids.iter().cloned())
        .collect();
    let top_level: Vec<_> = stages
        .iter()
        .filter(|s| !body.contains(&s.id))
        .map(|s| s.id.clone())
        .collect();
    if order.len() != top_level.len() {
        return Err("stage graph has a cycle in depends_on (not allowed outside loop bodies)".to_owned());
    }

    // Detect cycles within depends_on among non-body using DFS on full depends_on
    // for non-loop-body stages only.
    if has_depends_cycle(stages, &body) {
        return Err("stage graph has a cycle in depends_on".to_owned());
    }
    Ok(())
}

fn has_depends_cycle(stages: &[OrchestrationStage], body: &std::collections::HashSet<String>) -> bool {
    use std::collections::HashMap;
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for s in stages {
        if body.contains(&s.id) {
            continue;
        }
        for d in &s.depends_on {
            if body.contains(d) {
                continue;
            }
            adj.entry(d.clone()).or_default().push(s.id.clone());
        }
    }
    let mut visiting = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();
    fn dfs(
        n: &str,
        adj: &HashMap<String, Vec<String>>,
        visiting: &mut std::collections::HashSet<String>,
        visited: &mut std::collections::HashSet<String>,
    ) -> bool {
        if visited.contains(n) {
            return false;
        }
        if !visiting.insert(n.to_owned()) {
            return true;
        }
        if let Some(nexts) = adj.get(n) {
            for x in nexts {
                if dfs(x, adj, visiting, visited) {
                    return true;
                }
            }
        }
        visiting.remove(n);
        visited.insert(n.to_owned());
        false
    }
    for s in stages {
        if body.contains(&s.id) {
            continue;
        }
        if dfs(&s.id, &adj, &mut visiting, &mut visited) {
            return true;
        }
    }
    false
}

/// Apply a DAG edit: replace stages after validation. Returns cleaned stages
/// (pending status) ready to persist as a template or plan skeleton.
pub fn apply_stage_edit(stages: Vec<OrchestrationStage>) -> Result<Vec<OrchestrationStage>, String> {
    validate_stage_dag(&stages)?;
    Ok(stages
        .into_iter()
        .map(|mut s| {
            s.status = OrchestrationStageStatus::Pending;
            s.tasks.clear();
            s.current_iteration = None;
            s.error_message = None;
            // Keep seed output_payload for control stages if present.
            s
        })
        .collect())
}

/// Build an executable plan from an edited/bundled stage list + task text.
#[must_use]
pub fn plan_from_stages(
    parent_run_id: AgentRunId,
    task: &str,
    stages: Vec<OrchestrationStage>,
    workflow_id: Option<String>,
    workflow_title: Option<String>,
) -> OrchestrationPlan {
    OrchestrationPlan {
        parent_run_id,
        subagents: vec![],
        pattern_ids: vec![],
        planning_rationale: format!(
            "Editable DAG workflow for task: {}",
            task.chars().take(120).collect::<String>()
        ),
        stages,
        workflow_id,
        workflow_title,
    }
}

/// Coalesce terminal orchestration status so Cancelled is never overwritten by a late outcome.
#[must_use]
pub fn coalesce_orchestration_status(
    already: app_models::OrchestrationStatus,
    outcome: app_models::OrchestrationStatus,
) -> app_models::OrchestrationStatus {
    use app_models::OrchestrationStatus;
    if already == OrchestrationStatus::Cancelled || outcome == OrchestrationStatus::Cancelled {
        OrchestrationStatus::Cancelled
    } else {
        outcome
    }
}

/// Merge durable row status with an in-memory session before upsert.
#[must_use]
pub fn merge_status_for_store(
    durable: Option<app_models::OrchestrationStatus>,
    incoming: app_models::OrchestrationStatus,
) -> app_models::OrchestrationStatus {
    match durable {
        Some(d) => coalesce_orchestration_status(d, incoming),
        None => incoming,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::AgentRunId;

    #[test]
    fn catalog_has_at_least_three_templates() {
        let list = list_bundled_workflows();
        assert!(list.len() >= 3);
        assert!(list.iter().any(|w| w.id == "multi-lens-review"));
        assert!(list.iter().any(|w| w.id == "deep-research"));
        assert!(list.iter().any(|w| w.id == "iterative-refine"));
    }

    #[test]
    fn each_catalog_template_materializes() {
        for meta in list_bundled_workflows() {
            let plan = plan_bundled_workflow(meta.id, AgentRunId::new(), "sample task")
                .unwrap_or_else(|| panic!("missing plan for {}", meta.id));
            assert!(
                plan_has_stages(&plan),
                "{} should have stages",
                meta.id
            );
            validate_stage_dag(&plan.stages).unwrap_or_else(|e| panic!("{}: {e}", meta.id));
        }
    }

    #[test]
    fn iterative_refine_contains_loop_with_cap() {
        let plan =
            plan_bundled_workflow("iterative-refine", AgentRunId::new(), "refine the design")
                .expect("plan");
        let loop_stage = plan
            .stages
            .iter()
            .find(|s| s.kind == OrchestrationStageKind::Loop)
            .expect("loop");
        assert_eq!(clamp_loop_max_iterations(loop_stage.max_iterations), 3);
        assert_eq!(loop_stage.body_stage_ids, vec!["critique".to_owned()]);
        assert_eq!(loop_stage.stop_flag_path.as_deref(), Some("pass"));
        // Top-level order should run draft, then loop, then polish — not critique alone first.
        let order = topological_stage_order(&plan.stages);
        assert!(order.contains(&"draft".to_owned()));
        assert!(order.contains(&"refine_loop".to_owned()));
        assert!(order.contains(&"polish".to_owned()));
        assert!(!order.contains(&"critique".to_owned()));
    }

    #[test]
    fn loop_stop_on_pass_flag_and_max_iter_bound() {
        assert!(!loop_should_stop(r#"{"pass":false}"#, Some("pass")));
        assert!(loop_should_stop(r#"{"pass":true,"summary":"ok"}"#, Some("pass")));
        assert!(loop_should_stop(
            "notes\n```json\n{\"pass\": true}\n```\n",
            Some("pass")
        ));
        assert_eq!(clamp_loop_max_iterations(None), 3);
        assert_eq!(clamp_loop_max_iterations(Some(0)), 1);
        assert_eq!(clamp_loop_max_iterations(Some(100)), 32);
        // Simulated continue then stop
        let mut stopped = false;
        let max = clamp_loop_max_iterations(Some(5));
        let payloads = vec![
            r#"{"pass":false}"#,
            r#"{"pass":false}"#,
            r#"{"pass":true}"#,
        ];
        let mut iter = 0u32;
        while iter < max {
            iter += 1;
            let p = payloads.get((iter - 1) as usize).copied().unwrap_or(r#"{"pass":false}"#);
            if loop_should_stop(p, Some("pass")) {
                stopped = true;
                break;
            }
        }
        assert!(stopped);
        assert_eq!(iter, 3);
        // Hit max without pass
        iter = 0;
        while iter < max {
            iter += 1;
            if loop_should_stop(r#"{"pass":false}"#, Some("pass")) {
                break;
            }
        }
        assert_eq!(iter, max);
    }

    #[test]
    fn validate_rejects_cycle_and_accepts_edited_dag() {
        let mut stages = plan_bundled_workflow("deep-research", AgentRunId::new(), "t")
            .unwrap()
            .stages;
        // Introduce illegal cycle: plan depends on report
        stages[0].depends_on = vec!["report".to_owned()];
        assert!(validate_stage_dag(&stages).is_err());

        let good = plan_bundled_workflow("multi-lens-review", AgentRunId::new(), "t")
            .unwrap()
            .stages;
        assert!(validate_stage_dag(&good).is_ok());

        // Edit: add a reduce after synthesize
        let mut edited = good;
        edited.push(base_stage(
            "extra",
            OrchestrationStageKind::Single,
            "Extra note",
            "doc-writer",
            "Summarize again: {upstream}".to_owned(),
            vec!["synthesize".to_owned()],
        ));
        let applied = apply_stage_edit(edited).expect("edit ok");
        assert_eq!(applied.len(), 4);
        assert!(applied.iter().any(|s| s.id == "extra"));
        let plan = plan_from_stages(
            AgentRunId::new(),
            "t",
            applied,
            Some("custom".to_owned()),
            Some("Custom".to_owned()),
        );
        assert_eq!(plan.workflow_id.as_deref(), Some("custom"));
        let order = topological_stage_order(&plan.stages);
        assert!(
            order.iter().position(|x| x == "synthesize").unwrap()
                < order.iter().position(|x| x == "extra").unwrap()
        );
    }

    #[test]
    fn foreach_expands_via_execute_path_handoff_shape() {
        let plan = plan_bundled_workflow("multi-lens-review", AgentRunId::new(), "review auth")
            .expect("bundled");
        let stages = plan.stages.clone();
        let lenses = &stages[1];
        let keyed = collect_upstream_payloads(&stages, &lenses.depends_on);
        let control = primary_upstream_control_payload(&stages, &lenses.depends_on);
        let tasks = expand_foreach_tasks(lenses, &control);
        assert_eq!(tasks.len(), 3);
        let tasks_from_keyed = expand_foreach_tasks(lenses, &keyed);
        assert_eq!(tasks_from_keyed.len(), 3);
    }

    #[test]
    fn reduce_assembles_upstream_task_summaries() {
        let mut stages = plan_bundled_workflow("multi-lens-review", AgentRunId::new(), "t")
            .unwrap()
            .stages;
        stages[1].tasks = expand_foreach_tasks(
            &stages[1],
            stages[0].output_payload.as_deref().unwrap(),
        );
        stages[1].tasks[0].output_summary = Some("finding A".to_owned());
        stages[1].tasks[1].output_summary = Some("finding B".to_owned());
        stages[1].output_payload = Some(merge_foreach_outputs(&stages[1].tasks));
        let upstream = assemble_reduce_upstream(&stages, &["lenses".to_owned()]);
        assert!(upstream.contains("finding A"));
        assert!(upstream.contains("finding B"));
    }

    #[test]
    fn coalesce_and_store_merge_preserve_cancelled() {
        use app_models::OrchestrationStatus;
        assert_eq!(
            coalesce_orchestration_status(
                OrchestrationStatus::Cancelled,
                OrchestrationStatus::Completed
            ),
            OrchestrationStatus::Cancelled
        );
        assert_eq!(
            merge_status_for_store(
                Some(OrchestrationStatus::Cancelled),
                OrchestrationStatus::Running
            ),
            OrchestrationStatus::Cancelled
        );
    }

    #[test]
    fn render_prompt_substitutes_placeholders() {
        let out = render_stage_prompt("T:{task}|U:{upstream}|I:{item}", "do it", "up", "item-1");
        assert_eq!(out, "T:do it|U:up|I:item-1");
    }
}
