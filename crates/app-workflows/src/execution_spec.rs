//! Build [`RunExecutionSpec`] from built-in agent roles.
//!
//! Product tool names only (`fs_read`, `git`, …). Specs are the single source
//! of truth for multi-agent tool isolation, model tier, thinking, and timeouts.

use app_models::{
    AgentDefinition, BuiltInAgent, ModelTier, RetryClass, RunExecutionSpec, ThinkingMode,
    WriteIsolation,
};

/// Build an execution spec for a built-in role name (or `default` fallback).
#[must_use]
pub fn spec_for_role_name(role: &str) -> RunExecutionSpec {
    let agent = match role {
        "explorer" => BuiltInAgent::Explorer,
        "planner" => BuiltInAgent::Planner,
        "worker" => BuiltInAgent::Worker,
        "reviewer" => BuiltInAgent::Reviewer,
        "security-reviewer" => BuiltInAgent::SecurityReviewer,
        "tester" => BuiltInAgent::Tester,
        "researcher" => BuiltInAgent::Researcher,
        "doc-writer" => BuiltInAgent::DocWriter,
        "default" => BuiltInAgent::Default,
        // Stage synthesizers map to strong/no-tools-ish reduce profile via planner tools.
        "reduce" | "synthesizer" => BuiltInAgent::Planner,
        _ => BuiltInAgent::Default,
    };
    spec_for_agent(agent)
}

/// Build an execution spec from a registered agent definition.
#[must_use]
pub fn spec_from_definition(def: &AgentDefinition) -> RunExecutionSpec {
    let tier = if def.model_tier != ModelTier::Balanced {
        def.model_tier
    } else {
        match def.default_model_policy.as_str() {
            "fast" => ModelTier::Fast,
            "strong" => ModelTier::Strong,
            _ => def.model_tier,
        }
    };
    RunExecutionSpec {
        role: def.name.clone(),
        allowed_tools: def.allowed_tools.clone(),
        model_tier: tier,
        thinking_mode: def.thinking_default,
        reasoning_effort: None,
        timeout_ms: def.timeout_ms.max(30_000),
        soft_timeout_ms: Some((def.timeout_ms * 3 / 4).max(20_000)),
        max_tool_steps: if def.allows_many_tools() { 16 } else { 10 },
        retry_class: if def.name == "worker" || def.name == "doc-writer" {
            RetryClass::IdempotentOnly
        } else {
            RetryClass::Transient
        },
        write_isolation: def.write_isolation,
    }
}

/// Build from a built-in agent enum.
#[must_use]
pub fn spec_for_agent(agent: BuiltInAgent) -> RunExecutionSpec {
    let def = crate::AgentRegistry::built_in_static(agent);
    spec_from_definition(&def)
}

/// Resolve Auto thinking using role heuristics.
#[must_use]
pub fn resolve_thinking(spec: &RunExecutionSpec) -> ThinkingMode {
    let prefer = matches!(
        (spec.model_tier, spec.role.as_str()),
        (ModelTier::Strong, _)
            | (_, "planner" | "reviewer" | "security-reviewer" | "reduce" | "synthesizer")
    ) && !matches!(spec.role.as_str(), "explorer" | "tester");
    spec.thinking_mode.resolve(prefer)
}

/// Prompt prefix that steers DeepSeek dual-mode without proprietary headers.
#[must_use]
pub fn thinking_prompt_directive(mode: ThinkingMode) -> &'static str {
    match mode {
        ThinkingMode::On => {
            "[Thinking: ON] Reason carefully before answering. Prefer thorough analysis."
        }
        ThinkingMode::Off => {
            "[Thinking: OFF] Respond directly and concisely. Skip extended chain-of-thought."
        }
        ThinkingMode::Auto => "",
    }
}

/// Map a model tier to preferred DeepSeek model names (ordered).
#[must_use]
pub fn deepseek_model_candidates(tier: ModelTier) -> &'static [&'static str] {
    match tier {
        ModelTier::Fast => &["deepseek-v4-flash", "deepseek-chat"],
        ModelTier::Strong => &["deepseek-v4-pro", "deepseek-reasoner", "deepseek-chat"],
        ModelTier::Balanced => &[],
    }
}

/// Map a model tier to Moonshot-style candidates.
#[must_use]
pub fn moonshot_model_candidates(tier: ModelTier) -> &'static [&'static str] {
    match tier {
        ModelTier::Fast => &["kimi-k2-turbo-preview", "moonshot-v1-8k"],
        ModelTier::Strong => &["kimi-k2-0711-preview", "moonshot-v1-128k"],
        ModelTier::Balanced => &[],
    }
}

/// OpenAI-family tier candidates.
#[must_use]
pub fn openai_model_candidates(tier: ModelTier) -> &'static [&'static str] {
    match tier {
        ModelTier::Fast => &["gpt-4.1-mini", "gpt-4.1-nano", "gpt-4o-mini"],
        ModelTier::Strong => &["gpt-4.1", "gpt-4o"],
        ModelTier::Balanced => &[],
    }
}

trait AgentDefinitionExt {
    fn allows_many_tools(&self) -> bool;
}

impl AgentDefinitionExt for AgentDefinition {
    fn allows_many_tools(&self) -> bool {
        self.allowed_tools.len() >= 5
            || self.allowed_tools.iter().any(|t| t == "shell_exec" || t == "fs_write")
    }
}

/// Whether write isolation requires ensuring a worktree exists.
#[must_use]
pub fn needs_worktree(spec: &RunExecutionSpec) -> bool {
    matches!(
        spec.write_isolation,
        WriteIsolation::PreferWorktree | WriteIsolation::RequireWorktree
    ) && spec.allows_writes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_cannot_write() {
        let spec = spec_for_role_name("explorer");
        assert!(spec.allows_tool("fs_read"));
        assert!(spec.allows_tool("git"));
        assert!(!spec.allows_tool("fs_write"));
        assert!(!spec.allows_tool("shell_exec"));
        assert!(!spec.allows_writes());
        assert_eq!(spec.model_tier, ModelTier::Fast);
    }

    #[test]
    fn worker_can_write() {
        let spec = spec_for_role_name("worker");
        assert!(spec.allows_tool("fs_write"));
        assert!(spec.allows_tool("shell_exec"));
        assert!(spec.allows_tool("git:write"));
        assert!(spec.allows_writes());
        assert_eq!(spec.model_tier, ModelTier::Strong);
        assert_eq!(spec.write_isolation, WriteIsolation::RequireWorktree);
    }

    #[test]
    fn reviewer_prefers_thinking_on() {
        let spec = spec_for_role_name("reviewer");
        assert_eq!(resolve_thinking(&spec), ThinkingMode::On);
        assert!(!spec.allows_tool("fs_write"));
    }

    #[test]
    fn single_agent_default_has_full_tools() {
        let spec = RunExecutionSpec::single_agent_default();
        assert!(spec.allows_tool("shell_exec"));
        assert!(spec.allows_tool("fs_write"));
    }
}
