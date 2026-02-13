//! File watcher for `.attractor/` directory.
//!
//! Watches for changes to `prd.md` and `spec.md` and pushes updates
//! via SSE to the document viewer.

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::extract::{Query};
use futures::stream::BoxStream;
use serde::{Serialize, Deserialize};
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
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
    ///
    /// File updates are broadcast via the channel for SSE streaming, and also
    /// persisted to SQLite via the provided database pool.
    ///
    /// # Arguments
    /// - `watch_dir`: Path to the .attractor directory to watch
    /// - `db`: SQLite connection pool for persisting document changes
    /// - `project_id`: Database ID of the project being watched
    pub fn new(
        watch_dir: PathBuf,
        db: sqlx::SqlitePool,
        project_id: i64,
    ) -> Result<Self, notify::Error> {
        use notify::{Event as NotifyEvent, RecursiveMode, Watcher};

        let (sender, _) = broadcast::channel::<DocumentUpdate>(16);
        let tx = sender.clone();

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

                        // Broadcast via channel for live SSE updates
                        let _ = tx.send(DocumentUpdate {
                            doc_type: doc_type.to_string(),
                            content: content.clone(),
                        });

                        // Persist to SQLite asynchronously
                        if let Some(content_str) = content {
                            let db_pool = db.clone();
                            let doc_type_str = doc_type.to_string();
                            let project_id_copy = project_id;

                            tokio::runtime::Handle::current().spawn(async move {
                                if let Err(e) = crate::server::db::upsert_document(
                                    &db_pool,
                                    project_id_copy,
                                    &doc_type_str,
                                    &content_str,
                                )
                                .await
                                {
                                    tracing::error!(
                                        "Failed to persist document {} to DB: {}",
                                        doc_type_str,
                                        e
                                    );
                                }
                            });
                        }
                    }
                }
            })?;

        // Create .attractor dir if it doesn't exist
        std::fs::create_dir_all(&watch_dir).ok();

        watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;
        tracing::info!("Watching {:?} for document changes (project_id: {})", watch_dir, project_id);

        Ok(DocumentWatcher {
            sender,
            _watcher: watcher,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DocumentUpdate> {
        self.sender.subscribe()
    }
}

#[derive(Deserialize)]
pub struct DocStreamParams {
    project_id: i64,
}

/// Get or create a watcher for a project from the shared state.
fn get_or_create_watcher(
    state: &crate::server::AppState,
    project_id: i64,
    folder_path: &str,
) -> Result<Arc<DocumentWatcher>, String> {
    let mut watchers = state.watchers.lock().unwrap();

    // If watcher already exists, return it
    if let Some(watcher) = watchers.get(&project_id) {
        return Ok(watcher.clone());
    }

    // Create new watcher for this project
    let watch_dir = PathBuf::from(folder_path).join(".attractor");
    let watcher = Arc::new(
        DocumentWatcher::new(watch_dir, state.db.clone(), project_id)
            .map_err(|e| format!("Failed to create document watcher: {}", e))?,
    );

    watchers.insert(project_id, watcher.clone());
    Ok(watcher)
}

/// SSE endpoint handler: `GET /api/documents/stream?project_id=<id>`
///
/// Sends initial document state from DB, then streams live updates for a specific project.
pub async fn document_stream(
    Query(params): Query<DocStreamParams>,
    axum::extract::State(state): axum::extract::State<crate::server::AppState>,
) -> Sse<BoxStream<'static, Result<Event, Infallible>>> {
    tracing::info!("Document SSE connection established for project_id: {}", params.project_id);

    // Look up the project to get its folder path
    let project = match crate::server::db::get_project(&state.db, params.project_id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to get project {}: {}", params.project_id, e);
            let error_event = vec![Ok(Event::default()
                .event("error")
                .data(format!("Project not found: {}", params.project_id)))];
            let stream = futures::stream::iter(error_event).boxed();
            return Sse::new(stream).keep_alive(KeepAlive::default());
        }
    };

    // Get or create watcher for this project
    let watcher = match get_or_create_watcher(&state, params.project_id, &project.folder_path) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to create watcher: {}", e);
            let error_event = vec![Ok(Event::default().event("error").data(e))];
            let stream = futures::stream::iter(error_event).boxed();
            return Sse::new(stream).keep_alive(KeepAlive::default());
        }
    };

    let rx = watcher.subscribe();

    // Load initial documents from database
    let initial_events = match crate::server::db::get_documents(&state.db, params.project_id).await {
        Ok(docs) => docs
            .into_iter()
            .map(|doc| DocumentUpdate {
                doc_type: doc.doc_type,
                content: Some(doc.content),
            })
            .collect::<Vec<_>>(),
        Err(e) => {
            tracing::error!("Failed to load initial documents: {}", e);
            Vec::new()
        }
    };

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
    let combined = initial_stream.chain(live_stream).boxed();
    Sse::new(combined).keep_alive(KeepAlive::default())
}
