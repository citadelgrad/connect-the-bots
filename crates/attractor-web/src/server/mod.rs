pub mod execute;

// Leptos server functions: available to both SSR and WASM (for RPC stubs)
pub mod projects;

// SSR-only modules (no client stubs needed)
#[cfg(feature = "ssr")]
pub mod db;
#[cfg(feature = "ssr")]
pub mod documents;
#[cfg(feature = "ssr")]
pub mod stream;
#[cfg(feature = "ssr")]
pub mod terminal;

#[cfg(feature = "ssr")]
use std::collections::HashMap;
#[cfg(feature = "ssr")]
use std::sync::{Arc, Mutex};

/// Shared application state accessible from Axum routes.
#[cfg(feature = "ssr")]
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub watchers: Arc<Mutex<HashMap<i64, Arc<documents::DocumentWatcher>>>>,
    pub terminal_sessions: terminal::TerminalSessions,
}
