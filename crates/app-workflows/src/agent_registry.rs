//! Agent registry for built-in and custom orchestrator agents.

use app_models::{AgentDefinition, BuiltInAgent, PermissionScope};
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

    #[allow(clippy::too_many_lines)]
    fn built_in_definition(agent: BuiltInAgent) -> AgentDefinition {
        match agent {
            BuiltInAgent::Default => AgentDefinition {
                name: "default".to_owned(),
                description: "General-purpose agent for mixed tasks.".to_owned(),
                system_instructions: "You are a helpful software engineering assistant.".to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "context.inspect".to_owned(),
                    "memory.search".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Run,
            },
            BuiltInAgent::Explorer => AgentDefinition {
                name: "explorer".to_owned(),
                description: "Explores files, code, and tools.".to_owned(),
                system_instructions:
                    "Explore the workspace to answer questions about structure and content."
                        .to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "git.status".to_owned(),
                    "git.diff".to_owned(),
                    "context.inspect".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Thread,
            },
            BuiltInAgent::Planner => AgentDefinition {
                name: "planner".to_owned(),
                description: "Breaks work into ordered sub-tasks.".to_owned(),
                system_instructions: "Create a concise, executable plan grounded in the real repo \
(read key paths first). Prefer steps that a worker can implement immediately. \
If the user also asked for a deliverable, structure the plan so the next role can finish it."
                    .to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "context.inspect".to_owned(),
                    "memory.search".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Thread,
            },
            BuiltInAgent::Worker => AgentDefinition {
                name: "worker".to_owned(),
                description: "Writes code and makes filesystem changes.".to_owned(),
                system_instructions: "Result-oriented implementer: produce the concrete deliverable \
(code, docs, PlantUML files, patches). Read the workspace, then write. Do not end with only a plan."
                    .to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "filesystem.write".to_owned(),
                    "git.stage".to_owned(),
                    "git.commit".to_owned(),
                    "terminal.execute".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Workspace,
            },
            BuiltInAgent::Reviewer => AgentDefinition {
                name: "reviewer".to_owned(),
                description: "Reviews code quality and correctness.".to_owned(),
                system_instructions: "Review the proposed changes for correctness and style."
                    .to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "git.diff".to_owned(),
                    "context.inspect".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Run,
            },
            BuiltInAgent::SecurityReviewer => AgentDefinition {
                name: "security-reviewer".to_owned(),
                description: "Focuses on security implications.".to_owned(),
                system_instructions: "Review the task and outputs for security risks.".to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "git.diff".to_owned(),
                    "context.inspect".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Run,
            },
            BuiltInAgent::Tester => AgentDefinition {
                name: "tester".to_owned(),
                description: "Runs tests and validates behavior.".to_owned(),
                system_instructions: "Run relevant tests and report results.".to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "terminal.execute".to_owned(),
                    "git.status".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Run,
            },
            BuiltInAgent::Researcher => AgentDefinition {
                name: "researcher".to_owned(),
                description: "Researches context and external information.".to_owned(),
                system_instructions: "Research the topic and summarize findings with citations."
                    .to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "context.inspect".to_owned(),
                    "memory.search".to_owned(),
                    "mcp.invoke.read".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Thread,
            },
            BuiltInAgent::DocWriter => AgentDefinition {
                name: "doc-writer".to_owned(),
                description: "Writes documentation.".to_owned(),
                system_instructions: "Write clear, concise documentation.".to_owned(),
                allowed_tools: vec![
                    "filesystem.read".to_owned(),
                    "filesystem.write".to_owned(),
                    "context.inspect".to_owned(),
                ],
                default_model_policy: "default".to_owned(),
                default_permission_scope: PermissionScope::Workspace,
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
    fn register_custom_agent() {
        let mut registry = AgentRegistry::new();
        registry.register(AgentDefinition {
            name: "custom".to_owned(),
            description: "A custom agent.".to_owned(),
            system_instructions: "Be custom.".to_owned(),
            allowed_tools: vec!["filesystem.read".to_owned()],
            default_model_policy: "default".to_owned(),
            default_permission_scope: PermissionScope::Run,
        });
        assert_eq!(registry.list().len(), 10);
    }
}
