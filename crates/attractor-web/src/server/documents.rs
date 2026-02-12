//! File watcher for `.attractor/` directory.
//!
//! Watches for changes to `prd.md` and `spec.md` and pushes updates
//! via SSE to the document viewer.

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::Serialize;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;

#[derive(Serialize, Clone, Debug)]
pub struct DocumentUpdate {
    pub doc_type: String,
    pub content: Option<String>,
}

/// Shared state for the document watcher.
pub struct DocumentWatcher {
    sender: broadcast::Sender<DocumentUpdate>,
    _watcher: notify::RecommendedWatcher,
}

impl DocumentWatcher {
    /// Start watching the `.attractor/` directory for PRD/Spec changes.
    pub fn new(watch_dir: PathBuf) -> Result<Self, notify::Error> {
        use notify::{Event as NotifyEvent, RecursiveMode, Watcher};

        let (sender, _) = broadcast::channel::<DocumentUpdate>(16);
        let tx = sender.clone();

        let watch_path = watch_dir.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: Result<NotifyEvent, notify::Error>| {
                if let Ok(event) = res {
                    // Only care about write/create events
                    if !matches!(
                        event.kind,
                        notify::EventKind::Modify(_) | notify::EventKind::Create(_)
                    ) {
                        return;
                    }

                    for path in &event.paths {
                        let filename = path.file_name().and_then(|f| f.to_str());
                        let doc_type = match filename {
                            Some("prd.md") => "prd",
                            Some("spec.md") => "spec",
                            _ => continue,
                        };

                        let content = std::fs::read_to_string(path).ok();
                        tracing::info!(
                            "Document updated: {} ({} bytes)",
                            doc_type,
                            content.as_ref().map_or(0, |c| c.len())
                        );

                        let _ = tx.send(DocumentUpdate {
                            doc_type: doc_type.to_string(),
                            content,
                        });
                    }
                }
            })?;

        // Create .attractor dir if it doesn't exist
        std::fs::create_dir_all(&watch_dir).ok();

        watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;
        tracing::info!("Watching {:?} for document changes", watch_dir);

        Ok(DocumentWatcher {
            sender,
            _watcher: watcher,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DocumentUpdate> {
        self.sender.subscribe()
    }
}

/// SSE endpoint handler: `GET /api/documents/stream`
///
/// Sends initial document state, then streams live updates.
pub async fn document_stream(
    axum::extract::State(state): axum::extract::State<crate::server::AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Document SSE connection established");

    let rx = state.doc_watcher.subscribe();

    // Send initial state for any existing documents
    let initial_events = load_initial_documents(&state.attractor_dir);

    let initial_stream = futures::stream::iter(initial_events.into_iter().map(|update| {
        let data = serde_json::to_string(&update).unwrap_or_default();
        Ok(Event::default().event("document_update").data(data))
    }));

    let live_stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|msg| async {
        match msg {
            Ok(update) => {
                let data = serde_json::to_string(&update).unwrap_or_default();
                Some(Ok(Event::default().event("document_update").data(data)))
            }
            Err(_) => None,
        }
    });

    use futures::StreamExt;
    Sse::new(initial_stream.chain(live_stream)).keep_alive(KeepAlive::default())
}

/// Load existing PRD/Spec files for initial SSE state.
fn load_initial_documents(attractor_dir: &Path) -> Vec<DocumentUpdate> {
    let mut updates = Vec::new();

    for (filename, doc_type) in &[("prd.md", "prd"), ("spec.md", "spec")] {
        let path = attractor_dir.join(filename);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                updates.push(DocumentUpdate {
                    doc_type: doc_type.to_string(),
                    content: Some(content),
                });
            }
        }
    }

    updates
}
