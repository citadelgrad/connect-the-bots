use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

#[cfg(feature = "ssr")]
use crate::server::generate_prd_spec;

#[cfg(not(feature = "ssr"))]
use leptos::server_fn::client::generate_prd_spec;

#[component]
pub fn ChatInput() -> impl IntoView {
    let (prompt, set_prompt) = signal(String::new());
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    // Create action for server function
    let generate_action = Action::new(|prompt: &String| {
        let prompt = prompt.clone();
        async move { generate_prd_spec(prompt).await }
    });

    let navigate = use_navigate();

    // Watch for action completion
    Effect::new(move || {
        if let Some(result) = generate_action.value().get() {
            match result {
                Ok(response) => {
                    tracing::info!("Successfully generated PRD and Spec");

                    // Navigate with session_id for streaming, or fallback to query params
                    let url = if let Some(session_id) = response.session_id {
                        // Streaming mode: navigate with session_id
                        format!("/editor?session_id={}", session_id)
                    } else {
                        // Fallback mode: navigate with URL-encoded content
                        let prd_encoded = urlencoding::encode(&response.prd);
                        let spec_encoded = urlencoding::encode(&response.spec);
                        format!("/editor?prd={}&spec={}", prd_encoded, spec_encoded)
                    };

                    navigate(&url, Default::default());
                }
                Err(e) => {
                    tracing::error!("Failed to generate: {:?}", e);
                    set_error_msg.set(Some(format!("Error: {}", e)));
                }
            }
        }
    });

    let on_submit = move |_| {
        let prompt_value = prompt.get();
        if !prompt_value.is_empty() {
            set_error_msg.set(None);
            generate_action.dispatch(prompt_value.clone());
            set_prompt.set(String::new());
        }
    };

    let is_loading = move || generate_action.pending().get();

    view! {
        <div class="chat-input-container">
            <textarea
                class="chat-input"
                placeholder="Enter your project prompt here..."
                rows="8"
                prop:value=move || prompt.get()
                prop:disabled=is_loading
                on:input=move |ev| {
                    set_prompt.set(event_target_value(&ev));
                }
            />
            <button
                class="submit-button"
                prop:disabled=is_loading
                on:click=on_submit
            >
                {move || if is_loading() {
                    "Generating..."
                } else {
                    "Generate PRD"
                }}
            </button>
            {move || error_msg.get().map(|msg| view! {
                <div class="error-message">
                    {msg}
                </div>
            })}
        </div>
    }
}
