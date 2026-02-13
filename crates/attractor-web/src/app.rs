use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::components::layout::ProjectView;
use crate::server::projects::list_open_projects;

#[component]
fn HomePage() -> impl IntoView {
    let projects = Resource::new(|| (), |_| list_open_projects());

    view! {
        <Suspense fallback=|| view! { <p>"Loading projects..."</p> }>
            {move || projects.get().map(|result| match result {
                Ok(projects) if !projects.is_empty() => {
                    let project = projects.into_iter().next().unwrap();
                    view! { <ProjectView project=project/> }.into_any()
                }
                Ok(_) => view! {
                    <div class="empty-state">
                        <p>"No open projects. Open a project folder to get started."</p>
                    </div>
                }.into_any(),
                Err(e) => view! {
                    <div class="error-state">
                        <p>{format!("Error loading projects: {e}")}</p>
                    </div>
                }.into_any(),
            })}
        </Suspense>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/attractor-web.css"/>
        <Title text="Attractor"/>
        <Router>
            <Routes fallback=|| "Page not found.".into_view()>
                <Route path=path!("/") view=HomePage/>
            </Routes>
        </Router>
    }
}
