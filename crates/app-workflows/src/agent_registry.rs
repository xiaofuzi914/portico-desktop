//! Agent registry for built-in and custom orchestrator agents.
//!
//! Tool names are **product** tool ids (`fs_read`, `git`, `shell_exec`, …),
//! matching [`autoagents_adapter::PorticoToolRegistry`].

use app_models::{
    AgentDefinition, BuiltInAgent, ModelTier, PermissionScope, ThinkingMode, WriteIsolation,
};
use std::collections::HashMap;

/// Registry of agent definitions available to the orchestrator.
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    agents: HashMap<String, AgentDefinition>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

const READ_FS: &[&str] = &["fs_list", "fs_read", "fs_search"];
const READ_GIT: &[&str] = &["git"];
const WEB: &[&str] = &["web_search", "web_fetch"];
const WRITE_FS: &[&str] = &["fs_write", "fs_edit"];

impl AgentRegistry {
    /// Create a registry pre-populated with built-in agent definitions.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self {
            agents: HashMap::new(),
        };
        for agent in [
            BuiltInAgent::Default,
            BuiltInAgent::Explorer,
            BuiltInAgent::Planner,
            BuiltInAgent::Worker,
            BuiltInAgent::Reviewer,
            BuiltInAgent::SecurityReviewer,
            BuiltInAgent::Tester,
            BuiltInAgent::Researcher,
            BuiltInAgent::DocWriter,
        ] {
            let def = Self::built_in_definition(agent);
            registry.agents.insert(def.name.clone(), def);
        }
        registry
    }

    /// Static helper used by execution-spec builders without allocating a registry.
    #[must_use]
    pub fn built_in_static(agent: BuiltInAgent) -> AgentDefinition {
        Self::built_in_definition(agent)
    }

    /// Return the definition for a built-in agent role.
    #[must_use]
    pub fn built_in(&self, agent: BuiltInAgent) -> AgentDefinition {
        Self::built_in_definition(agent)
    }

    /// Register a custom agent definition.
    pub fn register(&mut self, def: AgentDefinition) {
        self.agents.insert(def.name.clone(), def);
    }

    /// Look up a registered agent by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<AgentDefinition> {
        self.agents.get(name).cloned()
    }

    /// List all registered agent definitions.
    #[must_use]
    pub fn list(&self) -> Vec<AgentDefinition> {
        self.agents.values().cloned().collect()
    }

    fn tools(parts: &[&[&str]]) -> Vec<String> {
        let mut out = Vec::new();
        for group in parts {
            for name in *group {
                if !out.iter().any(|existing| existing == name) {
                    out.push((*name).to_owned());
                }
            }
        }
        out
    }

    #[allow(clippy::too_many_lines)]
    fn built_in_definition(agent: BuiltInAgent) -> AgentDefinition {
        match agent {
            BuiltInAgent::Default => AgentDefinition {
                name: "default".to_owned(),
                description: "General-purpose agent for mixed tasks.".to_owned(),
                system_instructions: "You are a helpful software engineering assistant.".to_owned(),
                allowed_tools: Self::tools(&[READ_FS, READ_GIT, WEB]),
                default_model_policy: "balanced".to_owned(),
                default_permission_scope: PermissionScope::Run,
                model_tier: ModelTier::Balanced,
                thinking_default: ThinkingMode::Auto,
                timeout_ms: 180_000,
                write_isolation: WriteIsolation::None,
            },
            BuiltInAgent::Explorer => AgentDefinition {
                name: "explorer".to_owned(),
                description: "Explores files, code, and tools.".to_owned(),
                system_instructions:
                    "Explore the workspace to answer questions about structure and content. Read-only."
                        .to_owned(),
                allowed_tools: Self::tools(&[READ_FS, READ_GIT, WEB]),
                default_model_policy: "fast".to_owned(),
                default_permission_scope: PermissionScope::Thread,
                model_tier: ModelTier::Fast,
                thinking_default: ThinkingMode::Off,
                timeout_ms: 120_000,
                write_isolation: WriteIsolation::None,
            },
            BuiltInAgent::Planner => AgentDefinition {
                name: "planner".to_owned(),
                description: "Breaks work into ordered sub-tasks.".to_owned(),
                system_instructions: "Create a concise, executable plan grounded in the real repo \
(read key paths first). Prefer steps that a worker can implement immediately. \
If the user also asked for a deliverable, structure the plan so the next role can finish it."
                    .to_owned(),
                allowed_tools: Self::tools(&[READ_FS]),
                default_model_policy: "strong".to_owned(),
                default_permission_scope: PermissionScope::Thread,
                model_tier: ModelTier::Strong,
                thinking_default: ThinkingMode::On,
                timeout_ms: 150_000,
                write_isolation: WriteIsolation::None,
            },
            BuiltInAgent::Worker => AgentDefinition {
                name: "worker".to_owned(),
                description: "Writes code and makes filesystem changes.".to_owned(),
                system_instructions: "Result-oriented implementer: produce the concrete deliverable \
(code, docs, PlantUML files, patches). Read the workspace, then write. Do not end with only a plan."
                    .to_owned(),
                allowed_tools: Self::tools(&[
                    READ_FS,
                    WRITE_FS,
                    READ_GIT,
                    &["git:write"],
                    &["shell_exec"],
                ]),
                default_model_policy: "strong".to_owned(),
                default_permission_scope: PermissionScope::Workspace,
                model_tier: ModelTier::Strong,
                thinking_default: ThinkingMode::Auto,
                timeout_ms: 240_000,
                write_isolation: WriteIsolation::RequireWorktree,
            },
            BuiltInAgent::Reviewer => AgentDefinition {
                name: "reviewer".to_owned(),
                description: "Reviews code quality and correctness.".to_owned(),
                system_instructions: "Review the proposed changes for correctness and style."
                    .to_owned(),
                allowed_tools: Self::tools(&[READ_FS, READ_GIT]),
                default_model_policy: "strong".to_owned(),
                default_permission_scope: PermissionScope::Run,
                model_tier: ModelTier::Strong,
                thinking_default: ThinkingMode::On,
                timeout_ms: 150_000,
                write_isolation: WriteIsolation::None,
            },
            BuiltInAgent::SecurityReviewer => AgentDefinition {
                name: "security-reviewer".to_owned(),
                description: "Focuses on security implications.".to_owned(),
                system_instructions: "Review the task and outputs for security risks.".to_owned(),
                allowed_tools: Self::tools(&[READ_FS, READ_GIT, &["web_fetch"]]),
                default_model_policy: "strong".to_owned(),
                default_permission_scope: PermissionScope::Run,
                model_tier: ModelTier::Strong,
                thinking_default: ThinkingMode::On,
                timeout_ms: 150_000,
                write_isolation: WriteIsolation::None,
            },
            BuiltInAgent::Tester => AgentDefinition {
                name: "tester".to_owned(),
                description: "Runs tests and validates behavior.".to_owned(),
                system_instructions: "Run relevant tests and report results.".to_owned(),
                allowed_tools: Self::tools(&[READ_FS, READ_GIT, &["shell_exec"]]),
                default_model_policy: "fast".to_owned(),
                default_permission_scope: PermissionScope::Run,
                model_tier: ModelTier::Fast,
                thinking_default: ThinkingMode::Off,
                timeout_ms: 180_000,
                write_isolation: WriteIsolation::PreferWorktree,
            },
            BuiltInAgent::Researcher => AgentDefinition {
                name: "researcher".to_owned(),
                description: "Researches context and external information.".to_owned(),
                system_instructions: "Research the topic and summarize findings with citations."
                    .to_owned(),
                allowed_tools: Self::tools(&[READ_FS, READ_GIT, WEB]),
                default_model_policy: "fast".to_owned(),
                default_permission_scope: PermissionScope::Thread,
                model_tier: ModelTier::Fast,
                thinking_default: ThinkingMode::Auto,
                timeout_ms: 150_000,
                write_isolation: WriteIsolation::None,
            },
            BuiltInAgent::DocWriter => AgentDefinition {
                name: "doc-writer".to_owned(),
                description: "Writes documentation.".to_owned(),
                system_instructions: "Write clear, concise documentation.".to_owned(),
                allowed_tools: Self::tools(&[READ_FS, WRITE_FS]),
                default_model_policy: "strong".to_owned(),
                default_permission_scope: PermissionScope::Workspace,
                model_tier: ModelTier::Strong,
                thinking_default: ThinkingMode::Off,
                timeout_ms: 180_000,
                write_isolation: WriteIsolation::PreferWorktree,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_all_built_ins() {
        let registry = AgentRegistry::new();
        let agents = registry.list();
        assert_eq!(agents.len(), 9);
        for agent in [
            BuiltInAgent::Default,
            BuiltInAgent::Explorer,
            BuiltInAgent::Planner,
            BuiltInAgent::Worker,
            BuiltInAgent::Reviewer,
            BuiltInAgent::SecurityReviewer,
            BuiltInAgent::Tester,
            BuiltInAgent::Researcher,
            BuiltInAgent::DocWriter,
        ] {
            assert_eq!(registry.built_in(agent).name, agent.to_string());
        }
    }

    #[test]
    fn product_tool_names_only() {
        let registry = AgentRegistry::new();
        for agent in registry.list() {
            for tool in &agent.allowed_tools {
                assert!(
                    !tool.contains("filesystem.") && !tool.contains("terminal."),
                    "legacy tool name {tool} on {}",
                    agent.name
                );
            }
        }
    }

    #[test]
    fn register_custom_agent() {
        let mut registry = AgentRegistry::new();
        registry.register(AgentDefinition {
            name: "custom".to_owned(),
            description: "A custom agent.".to_owned(),
            system_instructions: "Be custom.".to_owned(),
            allowed_tools: vec!["fs_read".to_owned()],
            default_model_policy: "balanced".to_owned(),
            default_permission_scope: PermissionScope::Run,
            model_tier: ModelTier::Balanced,
            thinking_default: ThinkingMode::Auto,
            timeout_ms: 120_000,
            write_isolation: WriteIsolation::None,
        });
        assert_eq!(registry.list().len(), 10);
    }
}
