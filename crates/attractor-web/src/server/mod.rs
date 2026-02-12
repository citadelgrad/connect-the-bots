pub mod execute;

// SSR-only modules (no client stubs needed)
#[cfg(feature = "ssr")]
pub mod documents;
#[cfg(feature = "ssr")]
pub mod stream;
#[cfg(feature = "ssr")]
pub mod terminal;

/// Shared application state accessible from Axum routes.
#[cfg(feature = "ssr")]
#[derive(Clone)]
pub struct AppState {
    pub doc_watcher: std::sync::Arc<documents::DocumentWatcher>,
    pub attractor_dir: std::path::PathBuf,
}
