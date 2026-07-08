//! Event bus abstraction and in-memory implementation for Portico.

use app_models::{AgentRun, AgentRunId, AgentRunStatus, Artifact, ThreadId};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use ts_rs::TS;

/// Events emitted by the Portico runtime to subscribers.
///
/// Every variant carries `run_id`, `thread_id`, and a `timestamp` so that
/// frontend consumers can always route and order events.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", content = "data")]
pub enum RuntimeEvent {
    /// A run started executing.
    RunStarted {
        /// Run that started.
        run: AgentRun,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A run's status changed.
    RunStatusChanged {
        /// Run that changed status.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// New status.
        status: AgentRunStatus,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// Streaming content delta for a message.
    MessageDelta {
        /// Run producing the message.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// New content fragment.
        content: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A message finished streaming.
    MessageCompleted {
        /// Run producing the message.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Full assembled message content.
        content: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A tool call was requested by the run.
    ToolRequested {
        /// Run requesting the tool.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Tool name.
        tool_name: String,
        /// Tool arguments.
        #[ts(type = "any")]
        arguments: serde_json::Value,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A tool call requires user approval.
    ToolApprovalRequired {
        /// Run requesting approval.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Approval request id.
        request_id: i64,
        /// Action to approve.
        action: String,
        /// Resource targeted by the action.
        resource: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A tool started executing.
    ToolStarted {
        /// Run executing the tool.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Tool name.
        tool_name: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A tool completed successfully.
    ToolCompleted {
        /// Run executing the tool.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Tool name.
        tool_name: String,
        /// Tool result payload.
        #[ts(type = "any")]
        result: serde_json::Value,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A tool execution failed.
    ToolFailed {
        /// Run executing the tool.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Tool name.
        tool_name: String,
        /// Error message.
        error: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// An artifact was created by the run.
    ArtifactCreated {
        /// Run that created the artifact.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Artifact metadata.
        artifact: Artifact,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A run failed with an error.
    RunFailed {
        /// Run that failed.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Error message.
        error: String,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
    /// A run completed successfully.
    RunCompleted {
        /// Run that completed.
        run_id: AgentRunId,
        /// Thread that owns the run.
        thread_id: ThreadId,
        /// Timestamp of the event.
        timestamp: DateTime<Utc>,
    },
}

impl RuntimeEvent {
    /// Return the run id associated with this event.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        match self {
            Self::RunStarted { run, .. } => run.id,
            Self::RunStatusChanged { run_id, .. }
            | Self::MessageDelta { run_id, .. }
            | Self::MessageCompleted { run_id, .. }
            | Self::ToolRequested { run_id, .. }
            | Self::ToolApprovalRequired { run_id, .. }
            | Self::ToolStarted { run_id, .. }
            | Self::ToolCompleted { run_id, .. }
            | Self::ToolFailed { run_id, .. }
            | Self::ArtifactCreated { run_id, .. }
            | Self::RunFailed { run_id, .. }
            | Self::RunCompleted { run_id, .. } => *run_id,
        }
    }

    /// Return the thread id associated with this event.
    #[must_use]
    pub const fn thread_id(&self) -> ThreadId {
        match self {
            Self::RunStarted { run, .. } => run.thread_id,
            Self::RunStatusChanged { thread_id, .. }
            | Self::MessageDelta { thread_id, .. }
            | Self::MessageCompleted { thread_id, .. }
            | Self::ToolRequested { thread_id, .. }
            | Self::ToolApprovalRequired { thread_id, .. }
            | Self::ToolStarted { thread_id, .. }
            | Self::ToolCompleted { thread_id, .. }
            | Self::ToolFailed { thread_id, .. }
            | Self::ArtifactCreated { thread_id, .. }
            | Self::RunFailed { thread_id, .. }
            | Self::RunCompleted { thread_id, .. } => *thread_id,
        }
    }

    /// Return the timestamp associated with this event.
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::RunStarted { timestamp, .. }
            | Self::RunStatusChanged { timestamp, .. }
            | Self::MessageDelta { timestamp, .. }
            | Self::MessageCompleted { timestamp, .. }
            | Self::ToolRequested { timestamp, .. }
            | Self::ToolApprovalRequired { timestamp, .. }
            | Self::ToolStarted { timestamp, .. }
            | Self::ToolCompleted { timestamp, .. }
            | Self::ToolFailed { timestamp, .. }
            | Self::ArtifactCreated { timestamp, .. }
            | Self::RunFailed { timestamp, .. }
            | Self::RunCompleted { timestamp, .. } => *timestamp,
        }
    }
}

/// Stream of runtime events.
#[async_trait]
pub trait EventStream: Send + Sync {
    /// Wait for the next event.
    ///
    /// # Errors
    ///
    /// Returns an error if the stream fails.
    async fn next(&mut self) -> Result<Option<RuntimeEvent>, app_models::AppError>;
}

/// Broadcasts runtime events to subscribers.
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publish an event to all subscribers.
    ///
    /// # Errors
    ///
    /// Returns an error if the event cannot be published.
    async fn publish(&self, event: RuntimeEvent) -> Result<(), app_models::AppError>;

    /// Subscribe to events for a specific run.
    ///
    /// # Errors
    ///
    /// Returns an error if a subscription cannot be created.
    fn subscribe(&self, run_id: AgentRunId) -> Result<Box<dyn EventStream>, app_models::AppError>;

    /// Subscribe to all runtime events.
    ///
    /// # Errors
    ///
    /// Returns an error if a subscription cannot be created.
    fn subscribe_all(&self) -> Result<Box<dyn EventStream>, app_models::AppError>;
}

/// In-memory event bus backed by a single tokio broadcast channel.
#[derive(Debug, Clone)]
pub struct MemoryEventBus {
    sender: broadcast::Sender<RuntimeEvent>,
}

impl MemoryEventBus {
    /// Create a new event bus with the given channel capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _receiver) = broadcast::channel(capacity);
        Self { sender }
    }
}

impl Default for MemoryEventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[async_trait]
impl EventBus for MemoryEventBus {
    async fn publish(&self, event: RuntimeEvent) -> Result<(), app_models::AppError> {
        // Broadcast errors only happen when there are no receivers; that is fine
        // for a fire-and-forget event bus.
        let _ = self.sender.send(event);
        Ok(())
    }

    fn subscribe(&self, run_id: AgentRunId) -> Result<Box<dyn EventStream>, app_models::AppError> {
        let receiver = self.sender.subscribe();
        Ok(Box::new(FilteredEventStream { receiver, run_id }))
    }

    fn subscribe_all(&self) -> Result<Box<dyn EventStream>, app_models::AppError> {
        let receiver = self.sender.subscribe();
        Ok(Box::new(GlobalEventStream { receiver }))
    }
}

struct FilteredEventStream {
    receiver: broadcast::Receiver<RuntimeEvent>,
    run_id: AgentRunId,
}

#[async_trait]
impl EventStream for FilteredEventStream {
    async fn next(&mut self) -> Result<Option<RuntimeEvent>, app_models::AppError> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    if event.run_id() == self.run_id {
                        return Ok(Some(event));
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(None),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Skip lagged events and continue.
                }
            }
        }
    }
}

struct GlobalEventStream {
    receiver: broadcast::Receiver<RuntimeEvent>,
}

#[async_trait]
impl EventStream for GlobalEventStream {
    async fn next(&mut self) -> Result<Option<RuntimeEvent>, app_models::AppError> {
        match self.receiver.recv().await {
            Ok(event) => Ok(Some(event)),
            Err(broadcast::error::RecvError::Closed | broadcast::error::RecvError::Lagged(_)) => {
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_event_bus_delivers_event_to_global_subscriber() {
        let bus = MemoryEventBus::new(16);
        let mut stream = bus.subscribe_all().expect("should subscribe");

        let run_id = AgentRunId::new();
        let thread_id = ThreadId::new();
        bus.publish(RuntimeEvent::RunCompleted {
            run_id,
            thread_id,
            timestamp: Utc::now(),
        })
        .await
        .expect("should publish");

        let event = stream.next().await.expect("should recv").expect("some event");
        assert_eq!(event.run_id(), run_id);
        assert_eq!(event.thread_id(), thread_id);
    }

    #[tokio::test]
    async fn memory_event_bus_filters_by_run_id() {
        let bus = MemoryEventBus::new(16);
        let target = AgentRunId::new();
        let other = AgentRunId::new();
        let thread_id = ThreadId::new();
        let mut stream = bus.subscribe(target).expect("should subscribe");

        bus.publish(RuntimeEvent::RunCompleted {
            run_id: other,
            thread_id,
            timestamp: Utc::now(),
        })
        .await
        .expect("should publish");
        bus.publish(RuntimeEvent::RunCompleted {
            run_id: target,
            thread_id,
            timestamp: Utc::now(),
        })
        .await
        .expect("should publish");

        let event = stream.next().await.expect("should recv").expect("some event");
        assert_eq!(event.run_id(), target);
    }
}
