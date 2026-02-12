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

    // Session state cache for browser reconnection
    // Stores recent events (up to 100) to replay on reconnect
    static ref SESSION_STATE: std::sync::RwLock<HashMap<String, Vec<String>>> =
        std::sync::RwLock::new(HashMap::new());
}

/// SSE endpoint handler: `/api/stream/{session_id}`
///
/// Clients connect to this endpoint to receive real-time events for a specific session.
/// The connection is kept alive with periodic keep-alive messages.
///
/// Phase 5: Sends state_sync event on connection to restore UI state on browser reconnect.
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

    // Send state_sync event with buffered history if available
    let history = {
        let state = SESSION_STATE.read().unwrap();
        state.get(&session_id).cloned()
    };

    // Convert broadcast receiver to stream with optional state_sync event
    let sid_for_stream = session_id.clone();

    let stream = if let Some(events) = history {
        if !events.is_empty() {
            tracing::info!(
                "Sending state_sync with {} events for session {}",
                events.len(),
                sid_for_stream
            );
            let sync_event = serde_json::json!({
                "type": "state_sync",
                "events": events,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });

            // Send state_sync event first, then chain broadcast stream
            let sync_event_text = serde_json::to_string(&sync_event).unwrap_or_default();
            futures::stream::once(async move { Ok(Event::default().data(sync_event_text)) })
                .chain(BroadcastStream::new(rx).filter_map(|msg| async move {
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
                }))
                .boxed()
        } else {
            // No history, just stream broadcast events
            BroadcastStream::new(rx)
                .filter_map(|msg| async move {
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
                })
                .boxed()
        }
    } else {
        // No history, just stream broadcast events
        BroadcastStream::new(rx)
            .filter_map(|msg| async move {
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
            })
            .boxed()
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Publish an event to a session's SSE stream.
///
/// If no clients are connected to the session, the event is silently dropped.
/// This is a fire-and-forget operation.
///
/// Phase 5: Also stores event in SESSION_STATE for browser reconnection.
pub fn publish_event(session_id: &str, event: String) {
    // Store event in session state cache (keep last 100 events)
    {
        let mut state = SESSION_STATE.write().unwrap();
        let events = state.entry(session_id.to_string()).or_default();
        events.push(event.clone());

        // Keep only last 100 events to prevent unbounded growth
        if events.len() > 100 {
            events.drain(0..events.len() - 100);
        }
    }

    // Publish to broadcast channel
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
///
/// Phase 5: Also cleans up SESSION_STATE cache.
pub fn cleanup_inactive_sessions() {
    let mut channels = CHANNELS.write().unwrap();
    let mut state = SESSION_STATE.write().unwrap();

    channels.retain(|session_id, sender| {
        let has_subscribers = sender.receiver_count() > 0;
        if !has_subscribers {
            tracing::info!("Cleaning up inactive session: {}", session_id);
            // Also remove from state cache
            state.remove(session_id);
        }
        has_subscribers
    });
}

/// Clear session state after pipeline completion.
///
/// Phase 5: Should be called when pipeline completes to free memory.
pub fn clear_session_state(session_id: &str) {
    let mut state = SESSION_STATE.write().unwrap();
    if state.remove(session_id).is_some() {
        tracing::info!("Cleared session state for: {}", session_id);
    }
}
