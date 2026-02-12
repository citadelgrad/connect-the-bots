//! WebSocket PTY bridge for embedded Claude Code terminal.
//!
//! Spawns `claude` in a PTY and bridges I/O over WebSocket.
//! Handles terminal resize events sent as JSON messages.

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
struct ResizeEvent {
    #[serde(rename = "type")]
    _event_type: String,
    cols: u16,
    rows: u16,
}

/// WebSocket upgrade handler for terminal connections.
pub async fn ws_terminal(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_terminal_socket)
}

async fn handle_terminal_socket(ws: WebSocket) {
    let pty_system = native_pty_system();

    let pty_pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("Failed to open PTY: {}", e);
            return;
        }
    };

    let mut cmd = CommandBuilder::new("claude");
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }

    let _child = match pty_pair.slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(e) => {
            tracing::error!("Failed to spawn claude: {}", e);
            return;
        }
    };

    // Drop slave so master sees EOF when child exits
    drop(pty_pair.slave);

    let reader = match pty_pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to clone PTY reader: {}", e);
            return;
        }
    };

    let writer = match pty_pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to take PTY writer: {}", e);
            return;
        }
    };

    let writer = Arc::new(Mutex::new(writer));
    let master: Arc<Mutex<Box<dyn MasterPty + Send>>> =
        Arc::new(Mutex::new(pty_pair.master));
    let reader = Arc::new(Mutex::new(reader));

    let (mut ws_sender, mut ws_receiver) = ws.split();

    // PTY stdout → WebSocket
    let reader_clone = reader.clone();
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
                    if ws_sender.send(Message::Binary(data.into())).await.is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    });

    // WebSocket → PTY stdin + resize
    let writer_clone = writer.clone();
    let master_clone = master.clone();
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

    tracing::info!("Terminal WebSocket session ended");
}
