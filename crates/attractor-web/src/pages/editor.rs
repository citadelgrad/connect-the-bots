use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use crate::components::markdown_pane::MarkdownPane;

#[cfg(feature = "hydrate")]
use gloo_net::eventsource::futures::EventSource;
#[cfg(feature = "hydrate")]
use serde::{Deserialize, Serialize};

/// Stream event from SSE endpoint
#[cfg(feature = "hydrate")]
#[derive(Serialize, Deserialize, Clone, Debug)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    text: String,
}

#[component]
pub fn EditorPage() -> impl IntoView {
    let query = use_query_map();

    // Check if we have a session_id for streaming
    let session_id = move || query.get().get("session_id").map(|s| s.to_string());

    // Reactive signals for live content
    let (prd_content, set_prd_content) = signal(String::new());
    let (spec_content, set_spec_content) = signal(String::new());
    let (is_streaming, set_is_streaming) = signal(false);

    // Initialize content from query params or prepare for streaming
    Effect::new(move || {
        if let Some(sid) = session_id() {
            // Streaming mode: connect to SSE endpoint
            tracing::info!("Editor page in streaming mode with session_id: {}", sid);
            set_is_streaming.set(true);

            #[cfg(feature = "hydrate")]
            {
                leptos::task::spawn_local(async move {
                    use futures::StreamExt as _;

                    let url = format!("/api/stream/{}", sid);
                    tracing::info!("Connecting to SSE: {}", url);

                    match EventSource::new(&url) {
                        Ok(mut es) => {
                            let mut accumulated_text = String::new();
                            let mut stream = es.subscribe("message").unwrap();

                            while let Some(Ok((_, msg))) = stream.next().await {
                                // Convert JsValue to string and parse event data
                                if let Some(data_str) = msg.data().as_string() {
                                    if let Ok(event) = serde_json::from_str::<StreamEvent>(&data_str) {
                                        if event.event_type == "text" {
                                            // Accumulate text chunks
                                            accumulated_text.push_str(&event.text);

                                            // Split PRD from Spec on-the-fly
                                            let (prd, spec) = split_prd_and_spec(&accumulated_text);
                                            set_prd_content.set(prd);
                                            set_spec_content.set(spec);
                                        } else if event.event_type == "complete" {
                                            tracing::info!("Streaming complete");
                                            set_is_streaming.set(false);
                                            break;
                                        }
                                    }
                                }
                            }

                            es.close();
                        }
                        Err(e) => {
                            tracing::error!("Failed to create EventSource: {:?}", e);
                            set_is_streaming.set(false);
                        }
                    }
                });
            }
        } else {
            // Fallback mode: load from query params (backward compatibility)
            let prd = query
                .get()
                .get("prd")
                .and_then(|s| urlencoding::decode(&s).ok().map(|cow| cow.into_owned()))
                .unwrap_or_else(|| {
                    "# No PRD Content\n\nNavigate from the prompt page to generate PRD content."
                        .to_string()
                });

            let spec = query
                .get()
                .get("spec")
                .and_then(|s| urlencoding::decode(&s).ok().map(|cow| cow.into_owned()))
                .unwrap_or_else(|| {
                    "# No Spec Content\n\nNavigate from the prompt page to generate spec content."
                        .to_string()
                });

            set_prd_content.set(prd);
            set_spec_content.set(spec);
        }
    });

    view! {
        <div class="editor-page">
            <h1>"PRD / Spec Editor"</h1>
            {move || {
                if is_streaming.get() {
                    view! { <div class="streaming-indicator">"🔴 Live streaming..."</div> }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}
            <div class="editor-container">
                <MarkdownPane
                    title="PRD (Product Requirements)"
                    content=move || prd_content.get()
                />
                <MarkdownPane
                    title="Technical Specification"
                    content=move || spec_content.get()
                />
            </div>
        </div>
    }
}

/// Split accumulated text into PRD and Spec based on markdown headers.
///
/// Looks for "# Technical Specification" header to separate the two documents.
/// Falls back to empty spec if header not found.
#[allow(dead_code)]
fn split_prd_and_spec(text: &str) -> (String, String) {
    // Look for "# Technical Specification" header (case-insensitive)
    let spec_markers = [
        "# Technical Specification",
        "# Technical Spec",
        "#Technical Specification",
        "## Technical Specification",
    ];

    for marker in &spec_markers {
        if let Some(pos) = text.to_lowercase().find(&marker.to_lowercase()) {
            let prd = text[..pos].trim().to_string();
            let spec = text[pos..].trim().to_string();
            return (prd, spec);
        }
    }

    // No spec marker found yet - return all as PRD
    (text.to_string(), String::new())
}
