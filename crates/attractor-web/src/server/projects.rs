use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use crate::server::db;
#[cfg(feature = "ssr")]
use sqlx::SqlitePool;
#[cfg(feature = "ssr")]
use std::path::{Path, PathBuf};

// Re-export database types for use in server functions (SSR only)
#[cfg(feature = "ssr")]
pub use crate::server::db::{CachedDoc, Project};

// Client-side type definitions (when not SSR)
#[cfg(not(feature = "ssr"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub folder_path: String,
    pub name: String,
}

#[cfg(not(feature = "ssr"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDoc {
    pub doc_type: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDocs {
    pub prd: Option<String>,
    pub spec: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

/// Ensure `.attractor/CLAUDE.md` exists in the project directory.
///
/// This file tells Claude Code about the PRD/SPEC document system so that when
/// a terminal session starts, Claude knows to write PRD and SPEC files to the
/// correct locations (`.attractor/prd.md` and `.attractor/spec.md`).
///
/// The file is only created if it doesn't already exist — user edits are preserved.
#[cfg(feature = "ssr")]
fn scaffold_attractor_config(project_dir: &Path) {
    use std::fs;

    let attractor_dir = project_dir.join(".attractor");
    let claude_md_path = attractor_dir.join("CLAUDE.md");

    // Don't overwrite if the user has already customized it
    if claude_md_path.exists() {
        return;
    }

    // Ensure .attractor/ exists
    if let Err(e) = fs::create_dir_all(&attractor_dir) {
        tracing::warn!("Failed to create .attractor directory: {}", e);
        return;
    }

    // Load templates if they exist in the attractor installation
    let prd_template = load_bundled_template("prd-template.md");
    let spec_template = load_bundled_template("spec-template.md");

    let mut content = String::from(ATTRACTOR_CLAUDE_MD_HEADER);

    if let Some(prd) = prd_template {
        content.push_str("\n\n## PRD Template\n\n");
        content.push_str("Use this structure when creating a PRD:\n\n");
        content.push_str("````markdown\n");
        content.push_str(&prd);
        content.push_str("\n````\n");
    }

    if let Some(spec) = spec_template {
        content.push_str("\n\n## Technical Spec Template\n\n");
        content.push_str("Use this structure when creating a technical spec:\n\n");
        content.push_str("````markdown\n");
        content.push_str(&spec);
        content.push_str("\n````\n");
    }

    match fs::write(&claude_md_path, content) {
        Ok(()) => tracing::info!("Created {}", claude_md_path.display()),
        Err(e) => tracing::warn!("Failed to write {}: {}", claude_md_path.display(), e),
    }
}

/// Try to load a template from the bundled templates directory.
///
/// Looks for templates relative to the attractor binary, then falls back to
/// common install locations.
#[cfg(feature = "ssr")]
fn load_bundled_template(filename: &str) -> Option<String> {
    use std::fs;

    // Try relative to the current executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check sibling templates/ directory (development layout)
            let candidates = [
                exe_dir.join("templates").join(filename),
                exe_dir.join("../templates").join(filename),
                exe_dir.join("../../templates").join(filename),
            ];
            for candidate in &candidates {
                if let Ok(content) = fs::read_to_string(candidate) {
                    return Some(content);
                }
            }
        }
    }

    // Try from current working directory (common during development)
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_template = cwd.join("templates").join(filename);
        if let Ok(content) = fs::read_to_string(cwd_template) {
            return Some(content);
        }
    }

    None
}

#[cfg(feature = "ssr")]
const ATTRACTOR_CLAUDE_MD_HEADER: &str = r#"# Attractor Project Configuration

## Document System (PRD & Technical Spec)

This project uses the **Attractor** document system. When asked to create planning
documents, you MUST write them to the correct file paths so the web UI can display
them in real time.

### File Locations

| Document | File Path | Description |
|----------|-----------|-------------|
| **PRD** | `.attractor/prd.md` | Product Requirements Document |
| **Technical Spec** | `.attractor/spec.md` | Technical Specification |

### Rules

1. **Always write PRDs to `.attractor/prd.md`** — the web UI watches this exact path
2. **Always write specs to `.attractor/spec.md`** — the web UI watches this exact path
3. Do NOT create PRD/spec files in `docs/` or any other location
4. Overwrite the file each time (the web UI tracks versions via the database)
5. Follow the templates below for document structure
6. When creating both documents, write the PRD first, then the spec
"#;

// Server function implementations (Leptos #[server] macro generates client stubs automatically)
mod ssr_impl {
    use super::*;
    use leptos::prelude::*;

    #[cfg(feature = "ssr")]
    use std::fs;

    /// List all open projects sorted by most recently used.
    #[server]
    pub async fn list_open_projects() -> Result<Vec<Project>, ServerFnError> {
        let pool = use_context::<SqlitePool>()
            .ok_or_else(|| ServerFnError::new("No database pool"))?;

        db::list_open_projects(&pool)
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to list projects: {}", e)))
    }

    /// Open a project at the given folder path.
    /// Validates that the path exists and is a directory, then upserts into DB.
    #[server]
    pub async fn open_project(folder_path: String) -> Result<Project, ServerFnError> {
        let pool = use_context::<SqlitePool>()
            .ok_or_else(|| ServerFnError::new("No database pool"))?;

        // Validate that the path exists and is a directory
        let path = PathBuf::from(&folder_path);
        if !path.exists() {
            return Err(ServerFnError::new(format!(
                "Path does not exist: {}",
                folder_path
            )));
        }

        if !path.is_dir() {
            return Err(ServerFnError::new(format!(
                "Path is not a directory: {}",
                folder_path
            )));
        }

        // Canonicalize the path to resolve symlinks and normalize
        let canonical_path = path
            .canonicalize()
            .map_err(|e| ServerFnError::new(format!("Failed to canonicalize path: {}", e)))?;

        let canonical_str = canonical_path
            .to_str()
            .ok_or_else(|| ServerFnError::new("Path contains invalid UTF-8"))?
            .to_string();

        // Ensure .attractor/CLAUDE.md exists with PRD/SPEC instructions
        scaffold_attractor_config(&canonical_path);

        // Upsert into database
        db::upsert_project(&pool, &canonical_str)
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to open project: {}", e)))
    }

    /// Close a project (mark as not open) without deleting its data.
    #[server]
    pub async fn close_project(project_id: i64) -> Result<(), ServerFnError> {
        let pool = use_context::<SqlitePool>()
            .ok_or_else(|| ServerFnError::new("No database pool"))?;

        db::close_project(&pool, project_id)
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to close project: {}", e)))
    }

    /// Get cached PRD and Spec documents for a project.
    #[server]
    pub async fn get_cached_documents(project_id: i64) -> Result<CachedDocs, ServerFnError> {
        let pool = use_context::<SqlitePool>()
            .ok_or_else(|| ServerFnError::new("No database pool"))?;

        let docs = db::get_documents(&pool, project_id)
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to fetch documents: {}", e)))?;

        let mut prd = None;
        let mut spec = None;

        for doc in docs {
            match doc.doc_type.as_str() {
                "prd" => prd = Some(doc.content),
                "spec" => spec = Some(doc.content),
                _ => {}
            }
        }

        Ok(CachedDocs { prd, spec })
    }

    /// List directory entries (only directories) for the folder picker browser.
    /// If path is empty, defaults to home directory.
    /// Returns parent (..) entry for navigation (except at filesystem root).
    /// Filters out hidden directories starting with `.` unless in a special list.
    #[server]
    pub async fn list_directory(path: String) -> Result<Vec<DirEntry>, ServerFnError> {
        let dir_path = if path.is_empty() {
            // Default to home directory
            std::env::var("HOME")
                .map(PathBuf::from)
                .map_err(|_| ServerFnError::new("HOME environment variable not set"))?
        } else {
            PathBuf::from(&path)
        };

        // Verify the path is a directory
        if !dir_path.is_dir() {
            return Err(ServerFnError::new(format!(
                "Path is not a directory: {}",
                path
            )));
        }

        let mut entries = Vec::new();

        // Add parent (..) entry unless we're at filesystem root
        if dir_path.parent().is_some() && dir_path.parent() != Some(Path::new("")) {
            if let Some(parent) = dir_path.parent() {
                if let Some(parent_str) = parent.to_str() {
                    entries.push(DirEntry {
                        name: "..".to_string(),
                        path: parent_str.to_string(),
                        is_dir: true,
                    });
                }
            }
        }

        // Read directory and collect only subdirectories
        match fs::read_dir(&dir_path) {
            Ok(read_dir) => {
                let mut sub_entries: Vec<DirEntry> = read_dir
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        let path = entry.path();

                        // Skip hidden directories (starting with .)
                        if let Some(name) = path.file_name() {
                            if let Some(name_str) = name.to_str() {
                                if name_str.starts_with('.') {
                                    return None;
                                }
                            }
                        }

                        // Only include directories
                        if path.is_dir() {
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let path_str = path.to_str().unwrap_or("").to_string();

                            Some(DirEntry {
                                name,
                                path: path_str,
                                is_dir: true,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                // Sort by name
                sub_entries.sort_by(|a, b| a.name.cmp(&b.name));
                entries.extend(sub_entries);
            }
            Err(e) => {
                return Err(ServerFnError::new(format!(
                    "Failed to read directory: {}",
                    e
                )))
            }
        }

        Ok(entries)
    }
}

// Re-export server functions (Leptos #[server] macro handles client/server split)
pub use ssr_impl::*;
