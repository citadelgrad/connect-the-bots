//! WebSocket PTY bridge for embedded Claude Code terminal with session persistence.
//!
//! # Session lifecycle
//!
//! 1. **First visit** (no `?session=` param): spawns `claude` in a new PTY,
//!    generates a UUID session_id, stores the PTY in a shared map, and sends
//!    `{"type":"session","session_id":"<uuid>"}` to the client. The client saves
//!    it in `sessionStorage`.
//!
//! 2. **Browser refresh** (reconnect with `?session=<id>`): looks up the existing
//!    PTY in the map and reattaches the WebSocket. The PTY never died, so the
//!    Claude conversation continues seamlessly with full scrollback.
//!
//! 3. **Server restart** (client sends `?session=<id>` but map is empty): spawns
//!    a fresh `claude` session (not `--continue`) to avoid hijacking any Claude
//!    Code session running in a separate terminal. A new session_id is issued.
//!
//! 4. **Idle cleanup**: sessions with no WebSocket attached are reaped after 30
//!    minutes. The reap runs opportunistically on each new connection.
//!
//! # Wire protocol
//!
//! The WebSocket carries a mix of JSON control messages and raw terminal bytes:
//!
//! - **Connection URL**: `ws://host/path?session=<id>&folder=<path>`
//!   - `session` (optional): reconnect to existing session by ID
//!   - `folder` (optional): working directory for new PTY sessions
//! - **Server → Client** (first msg): `{"type":"session","session_id":"..."}`
//! - **Server → Client** (ongoing): binary frames of PTY stdout
//! - **Client → Server**: binary frames of keyboard input
//! - **Client → Server**: `{"type":"resize","cols":N,"rows":N}` on terminal resize

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A long-lived PTY session that survives WebSocket disconnects.
pub struct TerminalSession {
    pub master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub reader: Arc<Mutex<Box<dyn Read + Send>>>,
    #[allow(dead_code)]
    child: Mutex<Box<dyn portable_pty::Child + Send>>,
    /// When the last WebSocket disconnected (None = currently connected).
    pub disconnected_at: Mutex<Option<Instant>>,
}

/// Thread-safe map of session_id → TerminalSession.
#[derive(Clone, Default)]
pub struct TerminalSessions {
    inner: Arc<Mutex<HashMap<String, Arc<TerminalSession>>>>,
}

impl TerminalSessions {
    pub fn get(&self, id: &str) -> Option<Arc<TerminalSession>> {
        self.inner.lock().unwrap().get(id).cloned()
    }

    pub fn insert(&self, id: String, session: Arc<TerminalSession>) {
        self.inner.lock().unwrap().insert(id, session);
    }

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }

    /// Remove sessions that have been disconnected longer than `max_idle`.
    pub fn reap(&self, max_idle: std::time::Duration) {
        let mut map = self.inner.lock().unwrap();
        let now = Instant::now();
        map.retain(|id, session| {
            let keep = match *session.disconnected_at.lock().unwrap() {
                Some(t) => now.duration_since(t) < max_idle,
                None => true, // still connected
            };
            if !keep {
                tracing::info!("Reaping idle terminal session {}", id);
            }
            keep
        });
    }
}

#[derive(Deserialize)]
struct ResizeEvent {
    #[serde(rename = "type")]
    _event_type: String,
    cols: u16,
    rows: u16,
}

#[derive(Deserialize)]
pub struct WsQuery {
    session: Option<String>,
    folder: Option<String>,
}

/// Spawn a new claude PTY in the specified working directory.
fn spawn_claude_pty(cwd: &std::path::Path) -> Result<TerminalSession, String> {
    let pty_system = native_pty_system();

    let pty_pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {e}"))?;

    let mut cmd = CommandBuilder::new("claude");
    cmd.cwd(cwd);

    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn claude: {e}"))?;

    drop(pty_pair.slave);

    let reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone PTY reader: {e}"))?;

    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to take PTY writer: {e}"))?;

    Ok(TerminalSession {
        master: Arc::new(Mutex::new(pty_pair.master)),
        writer: Arc::new(Mutex::new(writer)),
        reader: Arc::new(Mutex::new(reader)),
        child: Mutex::new(child),
        disconnected_at: Mutex::new(None),
    })
}

/// WebSocket upgrade handler for terminal connections.
pub async fn ws_terminal(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<super::AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_socket(socket, query, state))
}

async fn handle_terminal_socket(ws: WebSocket, query: WsQuery, state: super::AppState) {
    let sessions = &state.terminal_sessions;

    // Reap stale sessions on every new connection
    sessions.reap(std::time::Duration::from_secs(30 * 60));

    // Resolve or create the session
    let (session_id, session) = if let Some(ref id) = query.session {
        if let Some(existing) = sessions.get(id) {
            tracing::info!("Reconnecting to terminal session {}", id);
            // Mark as connected
            *existing.disconnected_at.lock().unwrap() = None;
            (id.clone(), existing)
        } else {
            // Session gone (server restarted) — start fresh to avoid
            // hijacking an existing Claude Code session running elsewhere.
            tracing::info!(
                "Session {} not found (server restarted), spawning fresh claude",
                id
            );

            // Determine the working directory
            let fallback_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
            let cwd = if let Some(ref folder_path) = query.folder {
                let path = std::path::Path::new(folder_path);
                if path.exists() && path.is_dir() {
                    path
                } else {
                    tracing::warn!("Invalid folder path: {}, falling back to current_dir", folder_path);
                    fallback_dir.as_path()
                }
            } else {
                fallback_dir.as_path()
            };

            match spawn_claude_pty(cwd) {
                Ok(s) => {
                    let session = Arc::new(s);
                    let new_id = uuid::Uuid::new_v4().to_string();
                    sessions.insert(new_id.clone(), session.clone());
                    (new_id, session)
                }
                Err(e) => {
                    tracing::error!("{}", e);
                    return;
                }
            }
        }
    } else {
        // Brand new session
        // Determine the working directory
        let fallback_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        let cwd = if let Some(ref folder_path) = query.folder {
            let path = std::path::Path::new(folder_path);
            if path.exists() && path.is_dir() {
                path
            } else {
                tracing::warn!("Invalid folder path: {}, falling back to current_dir", folder_path);
                fallback_dir.as_path()
            }
        } else {
            fallback_dir.as_path()
        };

        match spawn_claude_pty(cwd) {
            Ok(s) => {
                let session = Arc::new(s);
                let id = uuid::Uuid::new_v4().to_string();
                sessions.insert(id.clone(), session.clone());
                tracing::info!("Created new terminal session {}", id);
                (id, session)
            }
            Err(e) => {
                tracing::error!("{}", e);
                return;
            }
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws.split();

    // Send the session_id to the client as first message
    let session_msg = serde_json::json!({
        "type": "session",
        "session_id": session_id,
    });
    if ws_sender
        .send(Message::Text(session_msg.to_string()))
        .await
        .is_err()
    {
        return;
    }

    // PTY stdout → WebSocket
    let reader_clone = session.reader.clone();
    let send_task = tokio::spawn(async move {
        loop {
            let reader_ref = reader_clone.clone();
            let result: Result<Vec<u8>, std::io::Error> =
                tokio::task::spawn_blocking(move || {
                    let mut r = reader_ref.lock().unwrap();
                    let mut buf = [0u8; 4096];
                    let n = r.read(&mut buf)?;
                    Ok(buf[..n].to_vec())
                })
                .await
                .unwrap_or_else(|_| Err(std::io::Error::other("join error")));

            match result {
                Ok(data) if !data.is_empty() => {
                    if ws_sender.send(Message::Binary(data)).await.is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    });

    // WebSocket → PTY stdin + resize
    let writer_clone = session.writer.clone();
    let master_clone = session.master.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Binary(data) => {
                    if let Ok(mut w) = writer_clone.lock() {
                        let _ = w.write_all(&data);
                    }
                }
                Message::Text(ref text) => {
                    if let Ok(resize) = serde_json::from_str::<ResizeEvent>(text) {
                        if let Ok(m) = master_clone.lock() {
                            let _ = m.resize(PtySize {
                                rows: resize.rows,
                                cols: resize.cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                    } else {
                        // Plain text input
                        if let Ok(mut w) = writer_clone.lock() {
                            let _ = w.write_all(text.as_bytes());
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }

    // Mark session as disconnected (but don't destroy it)
    *session.disconnected_at.lock().unwrap() = Some(Instant::now());
    tracing::info!("WebSocket disconnected from session {}, PTY kept alive", session_id);
}
