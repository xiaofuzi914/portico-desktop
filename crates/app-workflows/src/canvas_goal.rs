//! Template-based goal decomposition for project canvases.
//!
//! MVP uses a deterministic stage template (no LLM). Callers may later swap in
//! a model-backed decomposer with the same `DecomposedStage` shape.

/// One stage under a user goal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecomposedStage {
    /// Short stage title.
    pub title: String,
    /// What this stage should produce.
    pub summary: String,
    /// Acceptance / done criteria.
    pub acceptance: String,
    /// Prompt seed for launching a single-agent or multi-role run.
    pub suggested_prompt: String,
    /// Preferred launch path.
    pub launch_mode: StageLaunchMode,
}

/// How a stage should be launched when the user clicks "run".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageLaunchMode {
    /// `send_message` single agent.
    Single,
    /// `start_orchestration` multi-role.
    MultiRole,
}

impl StageLaunchMode {
    /// Persistable / payload string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::MultiRole => "multi-role",
        }
    }
}

impl TryFrom<&str> for StageLaunchMode {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "single" => Ok(Self::Single),
            "multi-role" | "multi_role" | "multirole" => Ok(Self::MultiRole),
            _ => Err(()),
        }
    }
}

/// Decompose a free-text goal into a fixed, result-oriented stage list.
///
/// Empty goals yield no stages.
#[must_use]
pub fn decompose_goal_template(goal: &str) -> Vec<DecomposedStage> {
    let goal = goal.trim();
    if goal.is_empty() {
        return Vec::new();
    }

    let base = goal.to_owned();
    vec![
        DecomposedStage {
            title: "调研与摸清现状".to_owned(),
            summary: format!("梳理与「{base}」相关的代码/文档现状与关键入口。"),
            acceptance: "有明确路径清单与结论；不确定处已标注。".to_owned(),
            suggested_prompt: format!(
                "【任务】围绕目标做调研\n【目标】{base}\n【角色】explorer\n\
【要求】中文结论先行；标注依据路径；不确定则标明不确定。先列目录与关键入口，再给结论。"
            ),
            launch_mode: StageLaunchMode::Single,
        },
        DecomposedStage {
            title: "方案与拆解".to_owned(),
            summary: format!("给出可执行的分步方案，服务目标：{base}"),
            acceptance: "有分步计划与优先级；标明依赖与风险。".to_owned(),
            suggested_prompt: format!(
                "【任务】制定可执行计划\n【目标】{base}\n【角色】planner\n\
【要求】中文结论先行；给出分步计划与验收点；仅计划、不要改代码，除非我明确要求落地。"
            ),
            launch_mode: StageLaunchMode::Single,
        },
        DecomposedStage {
            title: "实现与落地".to_owned(),
            summary: format!("按方案推进实现：{base}"),
            acceptance: "有具体改动/产物路径或可粘贴交付物。".to_owned(),
            suggested_prompt: format!(
                "【任务】实现并落地\n【目标】{base}\n【角色】explorer → worker\n\
【要求】结果导向闭环：先读再改；交付具体文件/路径；中文结论先行。"
            ),
            launch_mode: StageLaunchMode::MultiRole,
        },
        DecomposedStage {
            title: "验证与收口".to_owned(),
            summary: format!("验证交付并收口文档/说明：{base}"),
            acceptance: "有验证方式与剩余风险说明。".to_owned(),
            suggested_prompt: format!(
                "【任务】验证与收口\n【目标】{base}\n【角色】tester\n\
【要求】检查实现是否满足目标；列出验证步骤与缺口；中文结论先行。"
            ),
            launch_mode: StageLaunchMode::Single,
        },
    ]
}

/// Build a launch prompt from a stage payload + goal title.
#[must_use]
pub fn compose_stage_launch_prompt(
    goal_title: &str,
    stage_title: &str,
    stage_summary: &str,
    acceptance: &str,
    suggested_prompt: &str,
) -> String {
    let suggested = suggested_prompt.trim();
    if !suggested.is_empty() {
        return suggested.to_owned();
    }
    format!(
        "【任务】{stage_title}\n【所属目标】{goal_title}\n【说明】{stage_summary}\n\
【验收】{acceptance}\n【要求】中文结论先行；标注依据路径；不确定则标明不确定。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_goal_yields_no_stages() {
        assert!(decompose_goal_template("   ").is_empty());
    }

    #[test]
    fn template_has_four_stages_with_prompts() {
        let stages = decompose_goal_template("完成项目结构扫描");
        assert_eq!(stages.len(), 4);
        assert!(stages.iter().all(|s| !s.suggested_prompt.is_empty()));
        assert_eq!(stages[2].launch_mode, StageLaunchMode::MultiRole);
        assert!(stages[0].suggested_prompt.contains("完成项目结构扫描"));
    }

    #[test]
    fn compose_prefers_suggested_prompt() {
        let p = compose_stage_launch_prompt("G", "S", "sum", "acc", "  custom  ");
        assert_eq!(p, "custom");
    }

    #[test]
    fn compose_falls_back_when_suggested_empty() {
        let p = compose_stage_launch_prompt("G", "S", "sum", "acc", "  ");
        assert!(p.contains("【任务】S"));
        assert!(p.contains("【所属目标】G"));
    }
}
