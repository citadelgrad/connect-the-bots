use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::components::layout::MainLayout;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/attractor-web.css"/>
        <Title text="Attractor"/>
        <Router>
            <Routes fallback=|| "Page not found.".into_view()>
                <Route path=path!("/") view=MainLayout/>
            </Routes>
        </Router>
    }
}
