use leptos::prelude::*;
use crate::components::chat_input::ChatInput;

#[component]
pub fn PromptPage() -> impl IntoView {
    view! {
        <div class="prompt-page">
            <h1>"Attractor - Planning Tool"</h1>
            <p>"Enter your project prompt to generate a PRD or specification document."</p>
            <ChatInput/>
        </div>
    }
}
