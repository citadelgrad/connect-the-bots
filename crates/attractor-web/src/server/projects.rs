use crate::server::db;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

// Re-export database types for use in server functions
pub use crate::server::db::{CachedDoc, Project};

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

#[cfg(feature = "ssr")]
mod ssr_impl {
    use super::*;
    use leptos::prelude::*;
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

// Re-export server functions for the frontend
#[cfg(feature = "ssr")]
pub use ssr_impl::*;
