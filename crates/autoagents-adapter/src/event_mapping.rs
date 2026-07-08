//! Converts `AutoAgents` protocol events into Portico runtime events.

use app_models::{AgentRun, AgentRunId, AgentRunStatus, ThreadId, WorkspaceId};
use app_runtime::RuntimeEvent;
use autoagents_protocol::Event;
use chrono::Utc;

/// Map an `AutoAgents` [`Event`] into a Portico [`RuntimeEvent`].
///
/// `workspace_id` is required because [`RuntimeEvent::RunStarted`] carries the
/// full [`AgentRun`] metadata. `response_buffer` accumulates text streamed via
/// [`Event::StreamChunk`] so that the final [`RuntimeEvent::MessageCompleted`]
/// can carry the assembled response.
#[allow(clippy::too_many_lines)]
pub fn map_autoagents_event(
    event: &Event,
    run_id: AgentRunId,
    thread_id: ThreadId,
    workspace_id: WorkspaceId,
    response_buffer: &mut String,
) -> Option<RuntimeEvent> {
    let timestamp = Utc::now();

    match event {
        Event::TaskStarted {
            actor_id: _,
            actor_name: _,
            task_description: _,
            ..
        } => Some(RuntimeEvent::RunStarted {
            run: AgentRun {
                id: run_id,
                thread_id,
                workspace_id,
                status: AgentRunStatus::Running,
                created_at: timestamp,
                started_at: Some(timestamp),
                completed_at: None,
            },
            timestamp,
        }),

        Event::TaskComplete { result, .. } => {
            // Append the final result to the response buffer so subscribers that
            // only listen for MessageCompleted still receive the content.
            if !result.is_empty() && response_buffer.is_empty() {
                response_buffer.push_str(result);
            }
            Some(RuntimeEvent::RunCompleted {
                run_id,
                thread_id,
                timestamp,
            })
        }

        Event::TaskError { error, .. } => Some(RuntimeEvent::RunFailed {
            run_id,
            thread_id,
            error: error.clone(),
            timestamp,
        }),

        Event::StreamChunk { chunk, .. } => match chunk {
            autoagents_protocol::StreamChunk::Text(text) => {
                response_buffer.push_str(text);
                Some(RuntimeEvent::MessageDelta {
                    run_id,
                    thread_id,
                    content: text.clone(),
                    timestamp,
                })
            }
            autoagents_protocol::StreamChunk::ToolUseStart { id, name, .. } => {
                Some(RuntimeEvent::ToolRequested {
                    run_id,
                    thread_id,
                    tool_name: name.clone(),
                    arguments: serde_json::json!({"id": id}),
                    timestamp,
                })
            }
            autoagents_protocol::StreamChunk::ToolUseInputDelta {
                index: _,
                partial_json,
            } => Some(RuntimeEvent::MessageDelta {
                run_id,
                thread_id,
                content: partial_json.clone(),
                timestamp,
            }),
            _ => None,
        },

        Event::TurnCompleted {
            final_turn: true, ..
        } => {
            let content = response_buffer.clone();
            response_buffer.clear();
            Some(RuntimeEvent::MessageCompleted {
                run_id,
                thread_id,
                content,
                timestamp,
            })
        }

        Event::ToolCallRequested {
            id: _,
            tool_name,
            arguments,
            ..
        } => Some(RuntimeEvent::ToolRequested {
            run_id,
            thread_id,
            tool_name: tool_name.clone(),
            arguments: parse_or_wrap_arguments(arguments),
            timestamp,
        }),

        Event::ToolCallCompleted {
            tool_name, result, ..
        } => Some(RuntimeEvent::ToolCompleted {
            run_id,
            thread_id,
            tool_name: tool_name.clone(),
            result: result.clone(),
            timestamp,
        }),

        Event::ToolCallFailed {
            tool_name, error, ..
        } => Some(RuntimeEvent::ToolFailed {
            run_id,
            thread_id,
            tool_name: tool_name.clone(),
            error: error.clone(),
            timestamp,
        }),

        _ => None,
    }
}

fn parse_or_wrap_arguments(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments)
        .unwrap_or_else(|_| serde_json::Value::String(arguments.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoagents_protocol::{ActorID, Event, StreamChunk, SubmissionId};

    fn ids() -> (AgentRunId, ThreadId, WorkspaceId) {
        (AgentRunId::new(), ThreadId::new(), WorkspaceId::new())
    }

    #[test]
    fn task_started_maps_to_run_started() {
        let (run_id, thread_id, workspace_id) = ids();
        let mut buffer = String::new();
        let event = Event::TaskStarted {
            sub_id: SubmissionId::new_v4(),
            actor_id: ActorID::new_v4(),
            actor_name: "agent".to_owned(),
            task_description: "do work".to_owned(),
        };
        let mapped = map_autoagents_event(&event, run_id, thread_id, workspace_id, &mut buffer);
        assert!(
            matches!(mapped, Some(RuntimeEvent::RunStarted { run, .. }) if run.id == run_id && run.thread_id == thread_id)
        );
    }

    #[test]
    fn task_complete_maps_to_run_completed() {
        let (run_id, thread_id, workspace_id) = ids();
        let mut buffer = String::new();
        let event = Event::TaskComplete {
            sub_id: SubmissionId::new_v4(),
            actor_id: ActorID::new_v4(),
            actor_name: "agent".to_owned(),
            result: "done".to_owned(),
        };
        let mapped = map_autoagents_event(&event, run_id, thread_id, workspace_id, &mut buffer);
        assert!(
            matches!(mapped, Some(RuntimeEvent::RunCompleted { run_id: rid, thread_id: tid, .. }) if rid == run_id && tid == thread_id)
        );
        assert_eq!(buffer, "done");
    }

    #[test]
    fn task_error_maps_to_run_failed() {
        let (run_id, thread_id, workspace_id) = ids();
        let mut buffer = String::new();
        let event = Event::TaskError {
            sub_id: SubmissionId::new_v4(),
            actor_id: ActorID::new_v4(),
            error: "boom".to_owned(),
        };
        let mapped = map_autoagents_event(&event, run_id, thread_id, workspace_id, &mut buffer);
        assert!(
            matches!(mapped, Some(RuntimeEvent::RunFailed { run_id: rid, thread_id: tid, error, .. }) if rid == run_id && tid == thread_id && error == "boom")
        );
    }

    #[test]
    fn stream_chunk_text_maps_to_message_delta_and_appends_buffer() {
        let (run_id, thread_id, workspace_id) = ids();
        let mut buffer = String::new();
        let event = Event::StreamChunk {
            sub_id: SubmissionId::new_v4(),
            chunk: StreamChunk::Text("hello".to_owned()),
        };
        let mapped = map_autoagents_event(&event, run_id, thread_id, workspace_id, &mut buffer);
        assert!(
            matches!(mapped, Some(RuntimeEvent::MessageDelta { run_id: rid, thread_id: tid, content, .. }) if rid == run_id && tid == thread_id && content == "hello")
        );
        assert_eq!(buffer, "hello");
    }

    #[test]
    fn final_turn_completed_maps_to_message_completed() {
        let (run_id, thread_id, workspace_id) = ids();
        let mut buffer = "assembled response".to_owned();
        let event = Event::TurnCompleted {
            sub_id: SubmissionId::new_v4(),
            actor_id: ActorID::new_v4(),
            turn_number: 1,
            final_turn: true,
        };
        let mapped = map_autoagents_event(&event, run_id, thread_id, workspace_id, &mut buffer);
        assert!(
            matches!(mapped, Some(RuntimeEvent::MessageCompleted { run_id: rid, thread_id: tid, content, .. }) if rid == run_id && tid == thread_id && content == "assembled response")
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn tool_call_requested_maps_to_tool_requested() {
        let (run_id, thread_id, workspace_id) = ids();
        let mut buffer = String::new();
        let event = Event::ToolCallRequested {
            sub_id: SubmissionId::new_v4(),
            actor_id: ActorID::new_v4(),
            id: "call_1".to_owned(),
            tool_name: "search".to_owned(),
            arguments: r#"{"q":"rust"}"#.to_owned(),
        };
        let mapped = map_autoagents_event(&event, run_id, thread_id, workspace_id, &mut buffer);
        assert!(
            matches!(mapped, Some(RuntimeEvent::ToolRequested { run_id: rid, thread_id: tid, tool_name, .. }) if rid == run_id && tid == thread_id && tool_name == "search")
        );
    }

    #[test]
    fn tool_call_completed_maps_to_tool_completed() {
        let (run_id, thread_id, workspace_id) = ids();
        let mut buffer = String::new();
        let event = Event::ToolCallCompleted {
            sub_id: SubmissionId::new_v4(),
            actor_id: ActorID::new_v4(),
            id: "call_1".to_owned(),
            tool_name: "search".to_owned(),
            result: serde_json::json!({"ok": true}),
        };
        let mapped = map_autoagents_event(&event, run_id, thread_id, workspace_id, &mut buffer);
        assert!(
            matches!(mapped, Some(RuntimeEvent::ToolCompleted { run_id: rid, thread_id: tid, tool_name, .. }) if rid == run_id && tid == thread_id && tool_name == "search")
        );
    }

    #[test]
    fn tool_call_failed_maps_to_tool_failed() {
        let (run_id, thread_id, workspace_id) = ids();
        let mut buffer = String::new();
        let event = Event::ToolCallFailed {
            sub_id: SubmissionId::new_v4(),
            actor_id: ActorID::new_v4(),
            id: "call_1".to_owned(),
            tool_name: "search".to_owned(),
            error: "not found".to_owned(),
        };
        let mapped = map_autoagents_event(&event, run_id, thread_id, workspace_id, &mut buffer);
        assert!(
            matches!(mapped, Some(RuntimeEvent::ToolFailed { run_id: rid, thread_id: tid, tool_name, error, .. }) if rid == run_id && tid == thread_id && tool_name == "search" && error == "not found")
        );
    }
}
