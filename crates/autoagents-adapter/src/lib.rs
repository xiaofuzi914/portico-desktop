//! Adapter boundary between `AutoAgents` provider primitives and Portico's
//! product-owned durable runtime.
//!
//! Only provider construction, event translation, model execution, and safe
//! tool schemas cross this crate. Run lifecycle, policy, approval, tool effects,
//! and recovery remain owned by `app-runtime`.

pub mod event_mapping;
pub mod executor;
pub mod mcp_bridge;
pub mod mock_llm;
pub mod provider_factory;
pub mod tool_adapter;

pub use event_mapping::map_autoagents_event;
pub use executor::AutoAgentsExecutor;
pub use mcp_bridge::McpToolAdapter;
pub use mock_llm::MockLlmProvider;
pub use provider_factory::{RegistryExecutorResolver, build_llm_provider, check_provider_health};
pub use tool_adapter::PorticoToolRegistry;
