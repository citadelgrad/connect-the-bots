use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::pages::{editor::EditorPage, execution::ExecutionPage, prompt::PromptPage};

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/attractor-web.css"/>
        <Title text="Attractor Web Interface"/>
        <Router>
            <main>
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=path!("/") view=PromptPage/>
                    <Route path=path!("/editor") view=EditorPage/>
                    <Route path=path!("/execute") view=ExecutionPage/>
                </Routes>
            </main>
        </Router>
    }
}
