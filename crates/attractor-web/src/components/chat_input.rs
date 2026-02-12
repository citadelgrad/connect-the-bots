use leptos::prelude::*;

#[component]
pub fn ChatInput() -> impl IntoView {
    let (prompt, set_prompt) = signal(String::new());

    let on_submit = move |_| {
        let prompt_value = prompt.get();
        if !prompt_value.is_empty() {
            tracing::info!("Prompt submitted: {}", prompt_value);
            // TODO: Phase 2 will integrate claude CLI here
            set_prompt.set(String::new());
        }
    };

    view! {
        <div class="chat-input-container">
            <textarea
                class="chat-input"
                placeholder="Enter your project prompt here..."
                rows="8"
                prop:value=move || prompt.get()
                on:input=move |ev| {
                    set_prompt.set(event_target_value(&ev));
                }
            />
            <button
                class="submit-button"
                on:click=on_submit
            >
                "Generate PRD"
            </button>
        </div>
    }
}
