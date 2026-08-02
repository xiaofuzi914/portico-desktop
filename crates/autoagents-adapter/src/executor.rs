//! Product-owned durable model/tool loop backed by `AutoAgents` LLM providers.

use crate::tool_adapter::PorticoToolRegistry;
use app_models::{
    AgentRunId, AppError, RunExecutionSpec, ThinkingMode, ThreadId, ToolInvocationStatus,
    WorkspaceId,
};
use app_runtime::{
    AgentExecutionOutcome, AgentExecutor, ApprovalBroker, EventBus, PolicyGate,
    PreparedToolRequest, RuntimeEvent, SafeToolExecutor, Storage, ToolGateOutcome,
};
use app_security::SecurityContext;
use async_trait::async_trait;
use autoagents_llm::{
    FunctionCall, LLMProvider, ToolCall,
    chat::{ChatMessage, ChatRole, MessageType, StreamChunk, Tool},
};
use futures::StreamExt;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

const MAX_TOOL_STEPS: i64 = 16;

/// Per-call token usage attached to timeline exchange records.
#[derive(Debug, Clone, Copy)]
struct TokenUsageRecord {
    input_tokens: u64,
    output_tokens: u64,
    /// True when usage was estimated locally (provider omitted metadata).
    estimated: bool,
}

/// Executor that checkpoints normalized chat messages before every side effect.
#[derive(Clone)]
pub struct AutoAgentsExecutor {
    llm: Arc<dyn LLMProvider>,
    tools: Arc<PorticoToolRegistry>,
    security: Arc<SecurityContext>,
    /// When set, only these tools may be advertised/executed (multi-agent roles).
    role_spec: Option<RunExecutionSpec>,
    thinking_mode: ThinkingMode,
}

impl AutoAgentsExecutor {
    /// Create an executor with the default product security policy.
    #[must_use]
    pub fn new(llm: Arc<dyn LLMProvider>, tools: Arc<PorticoToolRegistry>) -> Self {
        Self {
            llm,
            tools,
            security: Arc::new(SecurityContext::new(
                Arc::new(app_security::PolicyPermissionEngine::default_rules()),
                Arc::new(app_security::DefaultCommandPolicy::new()),
                Arc::new(app_security::DefaultNetworkPolicy::new()),
                Arc::new(app_security::MemoryAuditLogger::new()),
            )),
            role_spec: None,
            thinking_mode: ThinkingMode::Auto,
        }
    }

    /// Use the same security context as the owning application runtime.
    #[must_use]
    pub fn with_security(mut self, security: Arc<SecurityContext>) -> Self {
        self.security = security;
        self
    }

    /// Restrict tools and apply thinking directive for a multi-agent role run.
    #[must_use]
    pub fn with_role_spec(mut self, spec: RunExecutionSpec, thinking: ThinkingMode) -> Self {
        self.role_spec = Some(spec);
        self.thinking_mode = thinking;
        self
    }

    fn enforce_role_tool(&self, tool_name: &str, arguments: &Value) -> Result<(), AppError> {
        let Some(spec) = &self.role_spec else {
            return Ok(());
        };
        if tool_name == "git" {
            let sub = arguments
                .get("subcommand")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let write_action = matches!(sub, "add" | "commit");
            if write_action {
                if !spec.allowed_tools.iter().any(|t| t == "git:write") {
                    return Err(AppError::PermissionDenied {
                        reason: format!(
                            "ROLE_TOOL_DENIED: role `{}` cannot use git write ({sub})",
                            spec.role
                        ),
                    });
                }
                return Ok(());
            }
            if !spec.allows_tool("git") {
                return Err(AppError::PermissionDenied {
                    reason: format!(
                        "ROLE_TOOL_DENIED: role `{}` cannot use tool `{tool_name}`",
                        spec.role
                    ),
                });
            }
            return Ok(());
        }
        if !spec.allows_tool(tool_name) {
            return Err(AppError::PermissionDenied {
                reason: format!(
                    "ROLE_TOOL_DENIED: role `{}` cannot use tool `{tool_name}`",
                    spec.role
                ),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl AgentExecutor for AutoAgentsExecutor {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    async fn execute(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        _workspace_id: WorkspaceId,
        message: &str,
        storage: Arc<dyn Storage>,
        event_bus: Arc<dyn EventBus>,
        token: CancellationToken,
    ) -> Result<AgentExecutionOutcome, AppError> {
        if token.is_cancelled() {
            return Ok(AgentExecutionOutcome::Completed(String::new()));
        }
        let gate = PolicyGate::new(storage.clone(), self.security.clone());
        let broker = ApprovalBroker::new(storage.clone());
        let safe_executor = SafeToolExecutor::new(gate.clone());
        let mut effective_thinking = self.thinking_mode;
        let mut thinking_degraded = false;
        let mut messages =
            load_messages_with_thinking(storage.as_ref(), run_id, message, effective_thinking)
                .await?;
        hydrate_completed_invocations(storage.as_ref(), run_id, &mut messages).await?;
        let tools = self.tools.llm_tools();
        let max_steps = self
            .role_spec
            .as_ref()
            .map_or(MAX_TOOL_STEPS, |s| i64::from(s.max_tool_steps).max(1));
        // Timeline sequence continues across approval resume.
        let mut seq = next_event_sequence(storage.as_ref(), run_id).await;

        for step in 0..max_steps {
            if token.is_cancelled() {
                return Ok(AgentExecutionOutcome::Completed(String::new()));
            }
            save_messages(storage.as_ref(), run_id, &messages, step).await?;
            let llm_started = Instant::now();
            let chat_result = stream_or_complete_chat(
                self.llm.as_ref(),
                &messages,
                &tools,
                run_id,
                thread_id,
                event_bus.as_ref(),
                &token,
            )
            .await;
            let (text, calls, usage) = match chat_result {
                Ok(v) => v,
                Err(error)
                    if !thinking_degraded
                        && matches!(effective_thinking, ThinkingMode::On | ThinkingMode::Auto)
                        && is_thinking_related_failure(&error) =>
                {
                    // One-shot degrade: turn thinking off and retry this step.
                    thinking_degraded = true;
                    effective_thinking = ThinkingMode::Off;
                    // Best-effort: surface degrade on durable snapshot.
                    // (Registry is not injected here; runner/snapshot may already exist.)
                    let _ = storage
                        .append_event(
                            run_id,
                            thread_id,
                            seq,
                            "thinking.degraded",
                            json!({
                                "from": self.thinking_mode.as_str(),
                                "to": "off",
                                "reason": error.to_string(),
                            }),
                        )
                        .await;
                    seq += 1;
                    // Rewrite first user message prefix if present.
                    if let Some(first) = messages.first_mut() {
                        first.content = apply_thinking_prefix(
                            &strip_thinking_prefix(&first.content),
                            ThinkingMode::Off,
                        );
                    }
                    stream_or_complete_chat(
                        self.llm.as_ref(),
                        &messages,
                        &tools,
                        run_id,
                        thread_id,
                        event_bus.as_ref(),
                        &token,
                    )
                    .await?
                }
                Err(error) => return Err(error),
            };
            let llm_duration_ms = elapsed_ms(llm_started);
            if let Some(usage) = usage {
                let _ = storage
                    .record_run_token_usage(run_id, usage.input_tokens, usage.output_tokens)
                    .await;
            }
            // Durable per-call ledger for the inspector timeline (request in / response out).
            record_llm_exchange(
                storage.as_ref(),
                run_id,
                thread_id,
                &mut seq,
                step,
                &messages,
                &tools,
                &text,
                &calls,
                usage,
                llm_duration_ms,
            )
            .await;
            if calls.is_empty() {
                // Final assistant turn: mark stream complete for UI, then durable persist via runner.
                if !text.trim().is_empty() {
                    let _ = event_bus
                        .publish(RuntimeEvent::MessageCompleted {
                            run_id,
                            thread_id,
                            content: text.clone(),
                            timestamp: chrono::Utc::now(),
                        })
                        .await;
                }
                save_messages(storage.as_ref(), run_id, &messages, step + 1).await?;
                return Ok(AgentExecutionOutcome::Completed(text));
            }

            messages.push(ChatMessage::assistant().content(text).tool_use(calls.clone()).build());
            save_messages(storage.as_ref(), run_id, &messages, step + 1).await?;

            for call in calls {
                let arguments: Value = match serde_json::from_str(&call.function.arguments) {
                    Ok(value) => value,
                    Err(error) => {
                        let result =
                            tool_error_result(format!("tool arguments are invalid JSON: {error}"));
                        record_tool_exchange(
                            storage.as_ref(),
                            run_id,
                            thread_id,
                            &mut seq,
                            step,
                            &call,
                            json!({ "raw": call.function.arguments }),
                            &result,
                            None,
                            0,
                        )
                        .await;
                        messages.push(tool_result_message(&call, &result)?);
                        continue;
                    }
                };
                event_bus
                    .publish(RuntimeEvent::ToolRequested {
                        run_id,
                        thread_id,
                        tool_name: call.function.name.clone(),
                        arguments: arguments.clone(),
                        timestamp: chrono::Utc::now(),
                    })
                    .await?;
                if let Err(error) = self.enforce_role_tool(&call.function.name, &arguments) {
                    let result = tool_error_result(error.to_string());
                    record_tool_exchange(
                        storage.as_ref(),
                        run_id,
                        thread_id,
                        &mut seq,
                        step,
                        &call,
                        arguments,
                        &result,
                        None,
                        0,
                    )
                    .await;
                    messages.push(tool_result_message(&call, &result)?);
                    continue;
                }
                let prepared = match prepared_request(&call, arguments.clone(), &self.tools) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        let result = tool_error_result(error.to_string());
                        record_tool_exchange(
                            storage.as_ref(),
                            run_id,
                            thread_id,
                            &mut seq,
                            step,
                            &call,
                            arguments,
                            &result,
                            None,
                            0,
                        )
                        .await;
                        messages.push(tool_result_message(&call, &result)?);
                        continue;
                    }
                };
                match gate.prepare(run_id, prepared).await {
                    Ok(ToolGateOutcome::Ready(invocation)) => {
                        let workspace_id = invocation.workspace_id;
                        let grant = broker.claim_ready(invocation.id).await?;
                        let tool_started = Instant::now();
                        let result = match dispatch_tool(
                            &safe_executor,
                            &self.tools,
                            &grant,
                            run_id,
                            workspace_id,
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(error) => {
                                let duration_ms = elapsed_ms(tool_started);
                                broker.fail(grant, &error.to_string()).await?;
                                if is_recoverable_tool_error(&error) {
                                    let result = tool_error_result(error.to_string());
                                    record_tool_exchange(
                                        storage.as_ref(),
                                        run_id,
                                        thread_id,
                                        &mut seq,
                                        step,
                                        &call,
                                        arguments,
                                        &result,
                                        Some("error"),
                                        duration_ms,
                                    )
                                    .await;
                                    messages.push(tool_result_message(&call, &result)?);
                                    continue;
                                }
                                return Err(error);
                            }
                        };
                        let duration_ms = elapsed_ms(tool_started);
                        broker.complete(grant, result.clone()).await?;
                        event_bus
                            .publish(RuntimeEvent::ToolCompleted {
                                run_id,
                                thread_id,
                                tool_name: call.function.name.clone(),
                                result: result.clone(),
                                timestamp: chrono::Utc::now(),
                            })
                            .await?;
                        record_tool_exchange(
                            storage.as_ref(),
                            run_id,
                            thread_id,
                            &mut seq,
                            step,
                            &call,
                            arguments,
                            &result,
                            Some("ok"),
                            duration_ms,
                        )
                        .await;
                        messages.push(tool_result_message(&call, &result)?);
                    }
                    Ok(ToolGateOutcome::WaitingApproval { approval, .. }) => {
                        event_bus
                            .publish(RuntimeEvent::ToolApprovalRequired {
                                run_id,
                                thread_id,
                                request_id: approval.id.0,
                                action: approval.action,
                                resource: approval.resource,
                                timestamp: chrono::Utc::now(),
                            })
                            .await?;
                        save_messages(storage.as_ref(), run_id, &messages, step + 1).await?;
                        return Ok(AgentExecutionOutcome::WaitingApproval);
                    }
                    Ok(ToolGateOutcome::Denied(invocation)) => {
                        let reason =
                            invocation.error.unwrap_or_else(|| "tool denied by policy".to_owned());
                        let result = tool_error_result(reason);
                        record_tool_exchange(
                            storage.as_ref(),
                            run_id,
                            thread_id,
                            &mut seq,
                            step,
                            &call,
                            arguments,
                            &result,
                            Some("denied"),
                            0,
                        )
                        .await;
                        messages.push(tool_result_message(&call, &result)?);
                    }
                    Err(error) if is_recoverable_tool_error(&error) => {
                        let result = tool_error_result(error.to_string());
                        record_tool_exchange(
                            storage.as_ref(),
                            run_id,
                            thread_id,
                            &mut seq,
                            step,
                            &call,
                            arguments,
                            &result,
                            Some("error"),
                            0,
                        )
                        .await;
                        messages.push(tool_result_message(&call, &result)?);
                    }
                    Err(error) => return Err(error),
                }
            }
            save_messages(storage.as_ref(), run_id, &messages, step + 1).await?;
        }

        // Hit the tool-step budget (typical for "scan the whole repo" tasks).
        // Do one final no-tools turn so the user still gets a partial answer
        // instead of a generic unexpected-error failure.
        if token.is_cancelled() {
            return Ok(AgentExecutionOutcome::Completed(String::new()));
        }
        messages.push(
            ChatMessage::user()
                .content(format!(
                    "You have reached the maximum of {MAX_TOOL_STEPS} tool rounds. \
Do NOT call any more tools. Summarize findings so far for the user in their language: \
what you inspected, key structure, concrete paths, and what is still incomplete. \
Suggest a smaller follow-up scope (one folder or subsystem)."
                ))
                .build(),
        );
        let empty_tools: [Tool; 0] = [];
        let (text, _calls, usage) = stream_or_complete_chat(
            self.llm.as_ref(),
            &messages,
            &empty_tools,
            run_id,
            thread_id,
            event_bus.as_ref(),
            &token,
        )
        .await?;
        if let Some(usage) = usage {
            let _ = storage
                .record_run_token_usage(run_id, usage.input_tokens, usage.output_tokens)
                .await;
        }
        let final_text = {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                format!(
                    "Reached the tool-step limit ({MAX_TOOL_STEPS}) while exploring the project. \
Ask for a smaller scope (one directory or one subsystem) and continue from there."
                )
            } else {
                text
            }
        };
        if !final_text.trim().is_empty() {
            let _ = event_bus
                .publish(RuntimeEvent::MessageCompleted {
                    run_id,
                    thread_id,
                    content: final_text.clone(),
                    timestamp: chrono::Utc::now(),
                })
                .await;
        }
        save_messages(storage.as_ref(), run_id, &messages, MAX_TOOL_STEPS + 1).await?;
        Ok(AgentExecutionOutcome::Completed(final_text))
    }
}

async fn load_messages(
    storage: &dyn Storage,
    run_id: AgentRunId,
    message: &str,
) -> Result<Vec<ChatMessage>, AppError> {
    match storage.load_agent_checkpoint(run_id).await? {
        Some(checkpoint) => {
            serde_json::from_value(checkpoint.messages).map_err(|e| AppError::Internal {
                message: format!("deserialize durable chat checkpoint failed: {e}"),
            })
        }
        None => Ok(vec![ChatMessage::user().content(message).build()]),
    }
}

async fn load_messages_with_thinking(
    storage: &dyn Storage,
    run_id: AgentRunId,
    message: &str,
    thinking: ThinkingMode,
) -> Result<Vec<ChatMessage>, AppError> {
    let prefixed = apply_thinking_prefix(message, thinking);
    load_messages(storage, run_id, &prefixed).await
}

fn thinking_prefix(mode: ThinkingMode) -> &'static str {
    match mode {
        ThinkingMode::On => {
            "[Thinking: ON] Reason carefully before answering. Prefer thorough analysis.\n\n"
        }
        ThinkingMode::Off => {
            "[Thinking: OFF] Respond directly and concisely. Skip extended chain-of-thought.\n\n"
        }
        ThinkingMode::Auto => "",
    }
}

fn apply_thinking_prefix(message: &str, mode: ThinkingMode) -> String {
    let prefix = thinking_prefix(mode);
    if prefix.is_empty() {
        message.to_owned()
    } else if message.starts_with("[Thinking:") {
        format!("{prefix}{}", strip_thinking_prefix(message))
    } else {
        format!("{prefix}{message}")
    }
}

fn strip_thinking_prefix(message: &str) -> String {
    let mut rest = message;
    for p in [
        "[Thinking: ON] Reason carefully before answering. Prefer thorough analysis.\n\n",
        "[Thinking: OFF] Respond directly and concisely. Skip extended chain-of-thought.\n\n",
    ] {
        if let Some(stripped) = rest.strip_prefix(p) {
            rest = stripped;
        }
    }
    rest.to_owned()
}

fn is_thinking_related_failure(error: &AppError) -> bool {
    let raw = error.to_string().to_ascii_lowercase();
    raw.contains("thinking")
        || raw.contains("reasoning")
        || raw.contains("tool")
        || raw.contains("timeout")
        || raw.contains("invalid")
        || raw.contains("400")
        || raw.contains("unstable")
        || raw.contains("content_filter")
}

async fn save_messages(
    storage: &dyn Storage,
    run_id: AgentRunId,
    messages: &[ChatMessage],
    step: i64,
) -> Result<(), AppError> {
    let value = serde_json::to_value(messages).map_err(|e| AppError::Internal {
        message: format!("serialize durable chat checkpoint failed: {e}"),
    })?;
    storage.save_agent_checkpoint(run_id, &value, step).await
}

async fn hydrate_completed_invocations(
    storage: &dyn Storage,
    run_id: AgentRunId,
    messages: &mut Vec<ChatMessage>,
) -> Result<(), AppError> {
    let existing: HashSet<String> = messages
        .iter()
        .filter_map(|message| match &message.message_type {
            MessageType::ToolResult(calls) => Some(calls.iter().map(|call| call.id.clone())),
            _ => None,
        })
        .flatten()
        .collect();
    for invocation in storage.list_tool_invocations(run_id).await? {
        if invocation.status != ToolInvocationStatus::Succeeded {
            continue;
        }
        let Some(call_id) = invocation.model_call_id else {
            continue;
        };
        if existing.contains(&call_id) {
            continue;
        }
        let result = invocation.result.unwrap_or(Value::Null);
        let call = ToolCall {
            id: call_id,
            call_type: "function".to_owned(),
            function: FunctionCall {
                name: invocation.tool_name,
                arguments: serde_json::to_string(&result).map_err(|e| AppError::Internal {
                    message: format!("serialize recovered tool result failed: {e}"),
                })?,
            },
        };
        messages.push(
            autoagents_llm::chat::ChatMessageBuilder::new(ChatRole::Tool)
                .tool_result(vec![call])
                .build(),
        );
    }
    Ok(())
}

fn prepared_request(
    call: &ToolCall,
    arguments: Value,
    tools: &PorticoToolRegistry,
) -> Result<PreparedToolRequest, AppError> {
    let name = call.function.name.as_str();
    let (action, resource) = match name {
        "fs_read" | "fs_list" | "fs_search" => {
            ("filesystem.read", required_string(&arguments, "path")?)
        }
        "fs_write" | "fs_edit" => ("filesystem.write", required_string(&arguments, "path")?),
        "git" => {
            let subcommand = required_string(&arguments, "subcommand")?;
            let repo = required_string(&arguments, "repo_path")?;
            match subcommand.as_str() {
                "status" | "diff" => ("git.read", repo),
                "add" | "commit" => ("git.write", repo),
                _ => {
                    return Err(AppError::PermissionDenied {
                        reason: "git subcommand must be status, diff, add, or commit".to_owned(),
                    });
                }
            }
        }
        "web_fetch" => ("network.fetch", required_string(&arguments, "url")?),
        "web_search" => ("network.search", required_string(&arguments, "query")?),
        "shell_exec" => ("shell.exec", required_string(&arguments, "command")?),
        other => {
            // Dynamic tools (MCP, plugins) registered on the shared registry.
            let Some(tool) = tools.get(other) else {
                return Err(AppError::PermissionDenied {
                    reason: format!(
                        "tool \"{other}\" is not registered; enable it under 能力中心 or use built-in tools"
                    ),
                });
            };
            let action = tool.policy_action().unwrap_or("mcp.invoke.write");
            (action, other.to_owned())
        }
    };
    Ok(PreparedToolRequest {
        model_call_id: Some(call.id.clone()),
        tool_name: call.function.name.clone(),
        tool_version: "safe-v1".to_owned(),
        action: action.to_owned(),
        resource,
        arguments,
        recovery: None,
    })
}

async fn dispatch_tool(
    safe_executor: &SafeToolExecutor,
    tools: &PorticoToolRegistry,
    grant: &app_runtime::ExecutionGrant,
    run_id: AgentRunId,
    workspace_id: WorkspaceId,
) -> Result<Value, AppError> {
    let name = grant.invocation().tool_name.as_str();
    if PorticoToolRegistry::is_durable_builtin(name) {
        return safe_executor.execute(grant).await;
    }
    // MCP / dynamic tools: PolicyGate already decided; invoke via registry adapter.
    let tool = tools.get(name).ok_or_else(|| AppError::PermissionDenied {
        reason: format!("tool \"{name}\" disappeared from the registry"),
    })?;
    let output = tool
        .invoke(app_tools::ToolInput {
            workspace_id,
            run_id,
            arguments: grant.invocation().arguments.clone(),
        })
        .await?;
    Ok(output.result)
}

fn tool_error_result(message: impl Into<String>) -> Value {
    let message = message.into();
    let message = message.strip_prefix("permission denied: ").unwrap_or(&message);
    json!({
        "ok": false,
        "error": message,
    })
}

const fn is_recoverable_tool_error(error: &AppError) -> bool {
    matches!(error, AppError::PermissionDenied { .. })
}

/// Stream one model turn (text + optional tool calls), publishing `MessageDelta` live.
///
/// Returns `(text, tool_calls, optional token usage)`.
/// Falls back to non-streaming `chat_with_tools` when streaming is unavailable.
#[allow(clippy::too_many_lines)]
async fn stream_or_complete_chat(
    llm: &dyn LLMProvider,
    messages: &[ChatMessage],
    tools: &[Tool],
    run_id: AgentRunId,
    thread_id: ThreadId,
    event_bus: &dyn EventBus,
    token: &CancellationToken,
) -> Result<(String, Vec<ToolCall>, Option<TokenUsageRecord>), AppError> {
    match llm.chat_stream_with_tools(messages, Some(tools), None).await {
        Ok(mut stream) => {
            let mut text = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut usage: Option<TokenUsageRecord> = None;
            while let Some(item) = stream.next().await {
                if token.is_cancelled() {
                    break;
                }
                let chunk = item.map_err(|e| classify_provider_chat_error(&e.to_string()))?;
                match chunk {
                    StreamChunk::Text(delta) => {
                        if delta.is_empty() {
                            continue;
                        }
                        text.push_str(&delta);
                        event_bus
                            .publish(RuntimeEvent::MessageDelta {
                                run_id,
                                thread_id,
                                content: delta,
                                timestamp: chrono::Utc::now(),
                            })
                            .await?;
                    }
                    StreamChunk::ToolUseComplete { tool_call, .. } => {
                        tool_calls.push(tool_call);
                    }
                    StreamChunk::ToolUseStart { name, .. } => {
                        let _ = event_bus
                            .publish(RuntimeEvent::ToolRequested {
                                run_id,
                                thread_id,
                                tool_name: name,
                                arguments: json!({}),
                                timestamp: chrono::Utc::now(),
                            })
                            .await;
                    }
                    StreamChunk::Usage(meta) => {
                        usage = Some(TokenUsageRecord {
                            input_tokens: u64::from(meta.prompt_tokens),
                            output_tokens: u64::from(meta.completion_tokens),
                            estimated: false,
                        });
                    }
                    StreamChunk::Done { .. }
                    | StreamChunk::ReasoningContent(_)
                    | StreamChunk::ToolUseInputDelta { .. } => {}
                }
            }
            // Heuristic fallback when the provider omits usage metadata.
            if usage.is_none() && (!text.is_empty() || !tool_calls.is_empty()) {
                let approx_out = (text.chars().count() as u64).div_ceil(3).max(1);
                let approx_in = estimate_prompt_tokens(messages);
                usage = Some(TokenUsageRecord {
                    input_tokens: approx_in,
                    output_tokens: approx_out,
                    estimated: true,
                });
            }
            Ok((text, tool_calls, usage))
        }
        Err(stream_err) => {
            tracing::warn!(
                error = %stream_err,
                "chat_stream_with_tools unavailable; falling back to chat_with_tools"
            );
            let response = llm
                .chat_with_tools(messages, Some(tools), None)
                .await
                .map_err(|e| classify_provider_chat_error(&e.to_string()))?;
            let text = response.text().unwrap_or_default();
            let calls = response.tool_calls().unwrap_or_default();
            if !text.is_empty() {
                let _ = event_bus
                    .publish(RuntimeEvent::MessageDelta {
                        run_id,
                        thread_id,
                        content: text.clone(),
                        timestamp: chrono::Utc::now(),
                    })
                    .await;
            }
            let usage = response
                .usage()
                .map(|u| TokenUsageRecord {
                    input_tokens: u64::from(u.prompt_tokens),
                    output_tokens: u64::from(u.completion_tokens),
                    estimated: false,
                })
                .or_else(|| {
                    if text.is_empty() && calls.is_empty() {
                        None
                    } else {
                        Some(TokenUsageRecord {
                            input_tokens: estimate_prompt_tokens(messages),
                            output_tokens: (text.chars().count() as u64).div_ceil(3).max(1),
                            estimated: true,
                        })
                    }
                });
            Ok((text, calls, usage))
        }
    }
}

fn estimate_prompt_tokens(messages: &[ChatMessage]) -> u64 {
    let chars: usize = messages.iter().map(|m| m.content.len()).sum();
    (chars as u64).div_ceil(3).max(1)
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

async fn next_event_sequence(storage: &dyn Storage, run_id: AgentRunId) -> i64 {
    match storage.list_run_events(run_id).await {
        Ok(events) => events.last().map_or(0, |e| e.sequence.saturating_add(1)),
        Err(error) => {
            tracing::warn!(error = %error, run_id = %run_id.0, "failed to load run events for sequence");
            0
        }
    }
}

fn tool_calls_json(calls: &[ToolCall]) -> Value {
    Value::Array(
        calls
            .iter()
            .map(|call| {
                let arguments = serde_json::from_str::<Value>(&call.function.arguments)
                    .unwrap_or_else(|_| Value::String(call.function.arguments.clone()));
                json!({
                    "id": call.id,
                    "type": call.call_type,
                    "function": {
                        "name": call.function.name,
                        "arguments": arguments,
                    }
                })
            })
            .collect(),
    )
}

fn tools_request_json(tools: &[Tool]) -> Value {
    serde_json::to_value(tools).unwrap_or_else(|_| {
        Value::Array(
            tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": tool.tool_type,
                        "function": {
                            "name": tool.function.name,
                            "description": tool.function.description,
                            "parameters": tool.function.parameters,
                        }
                    })
                })
                .collect(),
        )
    })
}

/// Persist one LLM API exchange for the inspector timeline.
#[allow(clippy::too_many_arguments)]
async fn record_llm_exchange(
    storage: &dyn Storage,
    run_id: AgentRunId,
    thread_id: ThreadId,
    sequence: &mut i64,
    step: i64,
    messages: &[ChatMessage],
    tools: &[Tool],
    text: &str,
    calls: &[ToolCall],
    usage: Option<TokenUsageRecord>,
    duration_ms: u64,
) {
    let request = json!({
        "messages": messages,
        "tools": tools_request_json(tools),
    });
    let response = json!({
        "text": text,
        "tool_calls": tool_calls_json(calls),
    });
    let usage_json = usage.map(|u| {
        json!({
            "input_tokens": u.input_tokens,
            "output_tokens": u.output_tokens,
            "total_tokens": u.input_tokens.saturating_add(u.output_tokens),
            "estimated": u.estimated,
        })
    });
    let payload = json!({
        "kind": "llm",
        "step": step,
        "request": request,
        "response": response,
        "usage": usage_json,
        "duration_ms": duration_ms,
    });
    match storage
        .append_event(run_id, thread_id, *sequence, "llm_exchange", payload)
        .await
    {
        Ok(_) => *sequence = sequence.saturating_add(1),
        Err(error) => {
            tracing::warn!(error = %error, run_id = %run_id.0, step, "failed to persist llm_exchange event");
        }
    }
}

/// Persist one tool invocation exchange for the inspector timeline.
#[allow(clippy::too_many_arguments)]
async fn record_tool_exchange(
    storage: &dyn Storage,
    run_id: AgentRunId,
    thread_id: ThreadId,
    sequence: &mut i64,
    step: i64,
    call: &ToolCall,
    arguments: Value,
    result: &Value,
    status: Option<&str>,
    duration_ms: u64,
) {
    let payload = json!({
        "kind": "tool",
        "step": step,
        "tool_name": call.function.name,
        "call_id": call.id,
        "request": {
            "tool_name": call.function.name,
            "call_id": call.id,
            "arguments": arguments,
        },
        "response": result,
        "status": status,
        "duration_ms": duration_ms,
    });
    match storage
        .append_event(run_id, thread_id, *sequence, "tool_exchange", payload)
        .await
    {
        Ok(_) => *sequence = sequence.saturating_add(1),
        Err(error) => {
            tracing::warn!(error = %error, run_id = %run_id.0, step, "failed to persist tool_exchange event");
        }
    }
}

/// Map raw provider HTTP failures into stable, user-mappable Internal codes.
fn classify_provider_chat_error(raw: &str) -> AppError {
    let lower = raw.to_lowercase();
    if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("deadline has elapsed")
    {
        return AppError::Internal {
            message: format!(
                "PROVIDER_TIMEOUT: model request timed out while waiting for a response ({raw})"
            ),
        };
    }
    if lower.contains("401") || lower.contains("unauthorized") || lower.contains("invalid api key")
    {
        return AppError::Internal {
            message: format!("PROVIDER_SECRET_MISSING: authentication failed ({raw})"),
        };
    }
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("too many requests")
    {
        return AppError::Internal {
            message: format!("PROVIDER_RATE_LIMITED: {raw}"),
        };
    }
    if lower.contains("context_length")
        || lower.contains("context length")
        || lower.contains("maximum context")
        || lower.contains("max_tokens")
            && (lower.contains("exceed") || lower.contains("too large") || lower.contains("limit"))
        || lower.contains("token limit")
        || lower.contains("too many tokens")
        || lower.contains("prompt is too long")
        || lower.contains("request too large")
        || lower.contains("content_length")
    {
        return AppError::Internal {
            message: format!(
                "PROVIDER_CONTEXT_OVERFLOW: model context window exceeded ({raw})"
            ),
        };
    }
    if lower.contains("503") || lower.contains("502") || lower.contains("overloaded") {
        return AppError::Internal {
            message: format!("PROVIDER_UNAVAILABLE: upstream provider unavailable ({raw})"),
        };
    }
    AppError::Internal {
        message: format!("provider chat failed: {raw}"),
    }
}

fn required_string(arguments: &Value, key: &str) -> Result<String, AppError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::PermissionDenied {
            reason: format!("tool argument '{key}' must be a string"),
        })
}

fn tool_result_message(call: &ToolCall, result: &Value) -> Result<ChatMessage, AppError> {
    let result_call = ToolCall {
        id: call.id.clone(),
        call_type: call.call_type.clone(),
        function: FunctionCall {
            name: call.function.name.clone(),
            arguments: serde_json::to_string(result).map_err(|e| AppError::Internal {
                message: format!("serialize tool result message failed: {e}"),
            })?,
        },
    };
    Ok(
        autoagents_llm::chat::ChatMessageBuilder::new(ChatRole::Tool)
            .tool_result(vec![result_call])
            .build(),
    )
}

#[cfg(test)]
mod classify_tests {
    use super::*;

    #[test]
    fn classifies_deepseek_style_timeout() {
        let err = classify_provider_chat_error(
            "HTTP Error: request timed out: error decoding response body for url (https://api.deepseek.com/chat/completions)",
        );
        match err {
            AppError::Internal { message } => {
                assert!(message.starts_with("PROVIDER_TIMEOUT:"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
