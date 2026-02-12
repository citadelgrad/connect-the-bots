//! Server-Sent Events (SSE) endpoint for streaming progress updates.
//!
//! Provides real-time event streaming to frontend clients via SSE.
//! Events are published to in-memory broadcast channels keyed by session_id.

use axum::{
    extract::Path,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{Stream, StreamExt};
use std::collections::HashMap;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

// In-memory broadcast channels keyed by session_id.
//
// Note: This is a Phase 3 implementation using in-memory state.
// For production, replace with Redis or a persistent message broker.
lazy_static::lazy_static! {
    static ref CHANNELS: std::sync::RwLock<HashMap<String, broadcast::Sender<String>>> =
        std::sync::RwLock::new(HashMap::new());
}

/// SSE endpoint handler: `/api/stream/{session_id}`
///
/// Clients connect to this endpoint to receive real-time events for a specific session.
/// The connection is kept alive with periodic keep-alive messages.
pub async fn stream_events(
    Path(session_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("SSE connection established for session: {}", session_id);

    // Get or create broadcast channel for this session
    let rx = {
        let mut channels = CHANNELS.write().unwrap();
        let sender = channels
            .entry(session_id.clone())
            .or_insert_with(|| broadcast::channel(100).0);
        sender.subscribe()
    };

    // Convert broadcast receiver to stream
    let stream = BroadcastStream::new(rx).filter_map(|msg| async move {
        match msg {
            Ok(text) => {
                tracing::debug!("Sending SSE event: {}", text);
                Some(Ok(Event::default().data(text)))
            }
            Err(e) => {
                tracing::warn!("Broadcast receiver error: {}", e);
                None
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Publish an event to a session's SSE stream.
///
/// If no clients are connected to the session, the event is silently dropped.
/// This is a fire-and-forget operation.
pub fn publish_event(session_id: &str, event: String) {
    let channels = CHANNELS.read().unwrap();
    if let Some(sender) = channels.get(session_id) {
        match sender.send(event) {
            Ok(count) => {
                tracing::debug!("Published event to {} receivers", count);
            }
            Err(e) => {
                tracing::warn!("Failed to publish event: {}", e);
            }
        }
    } else {
        tracing::debug!("No channel found for session_id: {}", session_id);
    }
}

/// Clean up old broadcast channels that have no active subscribers.
///
/// Should be called periodically (e.g., every hour) to prevent memory leaks.
/// In Phase 4+, replace with TTL-based cleanup or persistent storage.
pub fn cleanup_inactive_sessions() {
    let mut channels = CHANNELS.write().unwrap();
    channels.retain(|session_id, sender| {
        let has_subscribers = sender.receiver_count() > 0;
        if !has_subscribers {
            tracing::info!("Cleaning up inactive session: {}", session_id);
        }
        has_subscribers
    });
}
