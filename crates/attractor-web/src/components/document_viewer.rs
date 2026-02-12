use leptos::prelude::*;

use crate::components::markdown_render::render_markdown;

#[cfg(feature = "hydrate")]
use gloo_net::eventsource::futures::EventSource;
#[cfg(feature = "hydrate")]
use serde::Deserialize;

/// SSE document update event
#[cfg(feature = "hydrate")]
#[derive(Deserialize, Clone, Debug)]
struct DocumentUpdate {
    doc_type: String,
    #[serde(default)]
    content: Option<String>,
}

/// Active tab in the document viewer
#[derive(Clone, Copy, PartialEq)]
enum DocTab {
    Prd,
    Spec,
}

/// Tabbed document viewer that subscribes to SSE at `/api/documents/stream`
/// for live updates as Claude Code writes PRD/Spec files.
#[component]
pub fn DocumentViewer<FP, FS>(
    on_prd_change: FP,
    on_spec_change: FS,
) -> impl IntoView
where
    FP: Fn(bool) + Copy + Send + Sync + 'static,
    FS: Fn(bool) + Copy + Send + Sync + 'static,
{
    let (active_tab, set_active_tab) = signal(DocTab::Prd);
    let (prd_content, set_prd_content) = signal(String::new());
    let (spec_content, set_spec_content) = signal(String::new());

    // Subscribe to document updates via SSE
    #[cfg(feature = "hydrate")]
    {
        Effect::new(move || {
            leptos::task::spawn_local(async move {
                use futures::StreamExt as _;

                let url = "/api/documents/stream";
                match EventSource::new(url) {
                    Ok(mut es) => {
                        // Subscribe to the "document_update" event type
                        let mut stream = es.subscribe("document_update").unwrap();

                        while let Some(Ok((_, msg))) = stream.next().await {
                            if let Some(data_str) = msg.data().as_string() {
                                if let Ok(update) =
                                    serde_json::from_str::<DocumentUpdate>(&data_str)
                                {
                                    let content = update.content.unwrap_or_default();
                                    let has_content = !content.is_empty();

                                    match update.doc_type.as_str() {
                                        "prd" => {
                                            set_prd_content.set(content);
                                            on_prd_change(has_content);
                                        }
                                        "spec" => {
                                            set_spec_content.set(content);
                                            on_spec_change(has_content);
                                            // Auto-switch to spec tab when it appears
                                            if has_content
                                                && active_tab.get_untracked() == DocTab::Prd
                                            {
                                                set_active_tab.set(DocTab::Spec);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }

                        es.close();
                    }
                    Err(e) => {
                        tracing::error!("Failed to connect to document SSE: {:?}", e);
                    }
                }
            });
        });
    }

    let prd_html = move || render_markdown(&prd_content.get());
    let spec_html = move || render_markdown(&spec_content.get());

    view! {
        <div class="document-viewer">
            <div class="doc-tabs">
                <button
                    class=move || if active_tab.get() == DocTab::Prd { "doc-tab active" } else { "doc-tab" }
                    on:click=move |_| set_active_tab.set(DocTab::Prd)
                >
                    "PRD"
                </button>
                <button
                    class=move || if active_tab.get() == DocTab::Spec { "doc-tab active" } else { "doc-tab" }
                    on:click=move |_| set_active_tab.set(DocTab::Spec)
                >
                    "Spec"
                </button>
            </div>

            <div class="doc-content">
                {move || match active_tab.get() {
                    DocTab::Prd => {
                        let html = prd_html();
                        if html.is_empty() {
                            view! {
                                <div class="doc-placeholder">
                                    <p>"No PRD yet. Use Claude Code to create one:"</p>
                                    <code>"\"Create a PRD for ...\""</code>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="markdown-rendered" inner_html=html></div>
                            }.into_any()
                        }
                    }
                    DocTab::Spec => {
                        let html = spec_html();
                        if html.is_empty() {
                            view! {
                                <div class="doc-placeholder">
                                    <p>"No Spec yet. Use Claude Code to create one:"</p>
                                    <code>"\"Now create the technical spec\""</code>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="markdown-rendered" inner_html=html></div>
                            }.into_any()
                        }
                    }
                }}
            </div>
        </div>
    }
}
