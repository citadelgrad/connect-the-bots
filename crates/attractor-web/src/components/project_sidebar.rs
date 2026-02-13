use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::web_sys;
use crate::components::folder_picker::FolderPicker;
use crate::server::projects::{close_project, Project};

/// ProjectSidebar component for managing open projects.
///
/// Displays a list of all open projects, allows switching between them,
/// adding new projects, or closing existing ones.
///
/// ## Props
/// - `projects`: Signal containing the list of open projects
/// - `active_project_id`: Signal containing the ID of the currently active project
#[component]
pub fn ProjectSidebar(
    projects: RwSignal<Vec<Project>>,
    active_project_id: RwSignal<Option<i64>>,
) -> impl IntoView {
    let (show_picker, set_show_picker) = signal(false);

    // Handle adding a new project
    let handle_project_selected = move |project: Project| {
        let mut current = projects.get();
        if !current.iter().any(|p| p.id == project.id) {
            current.push(project.clone());
            projects.set(current);
        }
        active_project_id.set(Some(project.id));
        set_show_picker.set(false);
    };

    // Handle closing a project
    let handle_close_project = move |project_id: i64| {
        set_show_picker.set(false);

        spawn_local({
            async move {
                let _ = close_project(project_id).await;

                // Remove from local list
                let mut current = projects.get();
                current.retain(|p| p.id != project_id);
                projects.set(current);

                // Update active project if needed
                if active_project_id.get() == Some(project_id) {
                    let projects_list = projects.get();
                    active_project_id.set(projects_list.first().map(|p| p.id));
                }
            }
        });
    };

    // Handle project selection
    let handle_select_project = move |project_id: i64| {
        active_project_id.set(Some(project_id));
    };

    view! {
        <div class="project-sidebar">
            <div class="sidebar-header">
                <button
                    class="sidebar-new-project"
                    on:click=move |_| set_show_picker.set(true)
                >
                    "+ New Project"
                </button>
            </div>

            <div class="sidebar-projects">
                {move || {
                    let current_projects = projects.get();
                    let active_id = active_project_id.get();

                    if current_projects.is_empty() {
                        view! {
                            <div class="sidebar-empty">
                                "No projects open. Add one to get started."
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <For
                                each=move || current_projects.clone()
                                key=|project| project.id
                                children=move |project: Project| {
                                    let project_id = project.id;
                                    let is_active = active_id == Some(project_id);
                                    let project_name = project.name.clone();
                                    let project_path = project.folder_path.clone();

                                    let handle_close = move |ev: web_sys::MouseEvent| {
                                        ev.stop_propagation();
                                        handle_close_project(project_id);
                                    };

                                    let class_name = if is_active {
                                        "project-entry active"
                                    } else {
                                        "project-entry"
                                    };

                                    view! {
                                        <div
                                            class=class_name
                                            on:click=move |_| handle_select_project(project_id)
                                            title=project_path
                                        >
                                            <div class="project-name">{project_name}</div>
                                            <button
                                                class="project-close"
                                                on:click=handle_close
                                                title="Close project"
                                            >
                                                {"\u{00D7}"}
                                            </button>
                                        </div>
                                    }
                                }
                            />
                        }.into_any()
                    }
                }}
            </div>

            {move || {
                if show_picker.get() {
                    view! {
                        <FolderPicker
                            on_select=handle_project_selected
                            on_close=move || set_show_picker.set(false)
                        />
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }
            }}
        </div>
    }
}
