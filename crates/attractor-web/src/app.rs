use leptos::prelude::*;
use leptos_meta::*;

use crate::components::{layout::ProjectView, project_sidebar::ProjectSidebar};
use crate::server::projects::{list_open_projects, Project};

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // Workspace-level signals for multi-project state
    let projects = RwSignal::new(Vec::<Project>::new());
    let active_project_id = RwSignal::new(Option::<i64>::None);

    // Load open projects from DB on mount
    let load_action = Resource::new(|| (), |_| list_open_projects());
    Effect::new(move || {
        if let Some(Ok(loaded)) = load_action.get() {
            projects.set(loaded.clone());
            // Auto-select the first project if available
            if let Some(first) = loaded.first() {
                active_project_id.set(Some(first.id));
            }
        }
    });

    view! {
        <Stylesheet id="leptos" href="/pkg/attractor-web.css"/>
        <Title text="Attractor"/>

        <Suspense fallback=|| view! { <p>"Loading projects..."</p> }>
            <div class="app-workspace">
                <ProjectSidebar projects=projects active_project_id=active_project_id />
                <div class="workspace-content">
                    {move || {
                        let loaded_projects = projects.get();
                        let active_id = active_project_id.get();

                        if loaded_projects.is_empty() {
                            view! {
                                <div class="empty-state">
                                    <p>"No open projects. Click '+ New Project' to get started."</p>
                                </div>
                            }
                            .into_any()
                        } else {
                            view! {
                                <For
                                    each=move || loaded_projects.clone()
                                    key=|project| project.id
                                    children=move |project: Project| {
                                        let is_active = move || active_id == Some(project.id);
                                        let project_id = project.id;

                                        view! {
                                            <div
                                                class="project-view-wrapper"
                                                style:display=move || {
                                                    if is_active() { "flex" } else { "none" }
                                                }
                                            >
                                                <ProjectView project=project.clone() />
                                            </div>
                                        }
                                    }
                                />
                            }
                            .into_any()
                        }
                    }}
                </div>
            </div>
        </Suspense>
    }
}
