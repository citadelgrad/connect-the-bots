//! Pipeline event system for observability.
//!
//! Emits [`PipelineEvent`]s via a [`tokio::sync::broadcast`] channel so that
//! external observers (loggers, metrics collectors, UI, etc.) can subscribe to
//! pipeline execution progress without coupling to the engine internals.

use serde::{Deserialize, Serialize};

/// Events emitted during pipeline execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipelineEvent {
    PipelineStarted {
        pipeline_name: String,
        node_count: usize,
    },
    PipelineCompleted {
        pipeline_name: String,
        completed_nodes: Vec<String>,
        duration_ms: u64,
    },
    PipelineFailed {
        pipeline_name: String,
        error: String,
    },
    StageStarted {
        node_id: String,
        handler_type: String,
    },
    StageCompleted {
        node_id: String,
        status: String,
        duration_ms: u64,
    },
    StageFailed {
        node_id: String,
        error: String,
    },
    StageRetrying {
        node_id: String,
        attempt: usize,
    },
    EdgeSelected {
        from_node: String,
        to_node: String,
        edge_label: Option<String>,
    },
    GoalGateChecked {
        node_id: String,
        satisfied: bool,
    },
    CheckpointSaved {
        node_id: String,
    },
    ContextUpdated {
        node_id: String,
        keys: Vec<String>,
    },
}

/// Event emitter wrapping a broadcast sender.
#[derive(Clone)]
pub struct EventEmitter {
    sender: tokio::sync::broadcast::Sender<PipelineEvent>,
}

impl EventEmitter {
    /// Create a new emitter with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity);
        Self { sender }
    }

    /// Emit an event to all current subscribers.
    ///
    /// If there are no active receivers the event is silently dropped.
    pub fn emit(&self, event: PipelineEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to events. Returns a broadcast receiver.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<PipelineEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emitter_sends_and_receives() {
        let emitter = EventEmitter::new(16);
        let mut rx = emitter.subscribe();

        emitter.emit(PipelineEvent::PipelineStarted {
            pipeline_name: "test".into(),
            node_count: 3,
        });

        let event = rx.recv().await.unwrap();
        match event {
            PipelineEvent::PipelineStarted {
                pipeline_name,
                node_count,
            } => {
                assert_eq!(pipeline_name, "test");
                assert_eq!(node_count, 3);
            }
            other => panic!("unexpected event: {:?}", other),
        }
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_event() {
        let emitter = EventEmitter::new(16);
        let mut rx1 = emitter.subscribe();
        let mut rx2 = emitter.subscribe();

        emitter.emit(PipelineEvent::CheckpointSaved {
            node_id: "n1".into(),
        });

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();

        // Both subscribers should get the same event content.
        let json1 = serde_json::to_string(&e1).unwrap();
        let json2 = serde_json::to_string(&e2).unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn emit_with_no_subscribers_does_not_panic() {
        let emitter = EventEmitter::new(16);
        // No subscriber — this must not panic.
        emitter.emit(PipelineEvent::PipelineFailed {
            pipeline_name: "oops".into(),
            error: "something went wrong".into(),
        });
    }

    #[test]
    fn event_serialization_round_trip() {
        let event = PipelineEvent::StageCompleted {
            node_id: "node_42".into(),
            status: "ok".into(),
            duration_ms: 123,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: PipelineEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            PipelineEvent::StageCompleted {
                node_id,
                status,
                duration_ms,
            } => {
                assert_eq!(node_id, "node_42");
                assert_eq!(status, "ok");
                assert_eq!(duration_ms, 123);
            }
            other => panic!("unexpected variant after round-trip: {:?}", other),
        }
    }
}
