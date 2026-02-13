use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;

/// Project stored in the database
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Project {
    pub id: i64,
    pub folder_path: String,
    pub name: String,
}

/// Cached document content
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedDoc {
    pub doc_type: String,
    pub content: String,
}

/// Initialize the SQLite database and return a connection pool.
///
/// Creates `~/.attractor/web.db` if it doesn't exist, along with the directory.
/// Runs schema creation (idempotent).
pub async fn init_db() -> Result<SqlitePool, sqlx::Error> {
    // Determine database path: ~/.attractor/web.db
    let home_dir = std::env::var("HOME")
        .expect("HOME environment variable not set");
    let attractor_dir = PathBuf::from(home_dir).join(".attractor");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&attractor_dir)
        .expect("Failed to create ~/.attractor directory");

    let db_path = attractor_dir.join("web.db");
    let db_url = format!("sqlite:{}", db_path.display());

    // Create connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    // Create tables (idempotent)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS projects (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            folder_path TEXT    NOT NULL UNIQUE,
            name        TEXT    NOT NULL,
            is_open     INTEGER NOT NULL DEFAULT 1,
            last_used   TEXT    NOT NULL DEFAULT (datetime('now')),
            created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS documents (
            project_id  INTEGER NOT NULL REFERENCES projects(id),
            doc_type    TEXT    NOT NULL,
            content     TEXT    NOT NULL DEFAULT '',
            updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (project_id, doc_type)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

/// Insert or reopen a project. Returns the project record.
///
/// If the project doesn't exist, it's created with the folder basename as the name.
/// If it exists, `is_open` is set to 1 and `last_used` is updated.
pub async fn upsert_project(
    pool: &SqlitePool,
    folder_path: &str,
) -> Result<Project, sqlx::Error> {
    // Extract project name from folder path
    let name = PathBuf::from(folder_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Insert if not exists
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO projects (folder_path, name)
        VALUES (?, ?)
        "#,
    )
    .bind(folder_path)
    .bind(&name)
    .execute(pool)
    .await?;

    // Update last_used and set is_open=1
    sqlx::query(
        r#"
        UPDATE projects
        SET is_open = 1, last_used = datetime('now')
        WHERE folder_path = ?
        "#,
    )
    .bind(folder_path)
    .execute(pool)
    .await?;

    // Fetch and return the project
    let project = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        SELECT id, folder_path, name
        FROM projects
        WHERE folder_path = ?
        "#,
    )
    .bind(folder_path)
    .fetch_one(pool)
    .await?;

    Ok(Project {
        id: project.0,
        folder_path: project.1,
        name: project.2,
    })
}

/// List all open projects, ordered by most recently used first.
pub async fn list_open_projects(pool: &SqlitePool) -> Result<Vec<Project>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        SELECT id, folder_path, name
        FROM projects
        WHERE is_open = 1
        ORDER BY last_used DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, folder_path, name)| Project {
            id,
            folder_path,
            name,
        })
        .collect())
}

/// Get a single project by ID.
pub async fn get_project(pool: &SqlitePool, project_id: i64) -> Result<Project, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        SELECT id, folder_path, name
        FROM projects
        WHERE id = ?
        "#,
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    Ok(Project {
        id: row.0,
        folder_path: row.1,
        name: row.2,
    })
}

/// Close a project (set is_open=0). Does not delete.
pub async fn close_project(pool: &SqlitePool, project_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE projects
        SET is_open = 0
        WHERE id = ?
        "#,
    )
    .bind(project_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Store or update document content for a project.
pub async fn upsert_document(
    pool: &SqlitePool,
    project_id: i64,
    doc_type: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO documents (project_id, doc_type, content, updated_at)
        VALUES (?, ?, ?, datetime('now'))
        "#,
    )
    .bind(project_id)
    .bind(doc_type)
    .bind(content)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get all cached documents for a project.
pub async fn get_documents(
    pool: &SqlitePool,
    project_id: i64,
) -> Result<Vec<CachedDoc>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT doc_type, content
        FROM documents
        WHERE project_id = ?
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(doc_type, content)| CachedDoc { doc_type, content })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a temporary database for testing
    async fn temp_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        // Create tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_path TEXT    NOT NULL UNIQUE,
                name        TEXT    NOT NULL,
                is_open     INTEGER NOT NULL DEFAULT 1,
                last_used   TEXT    NOT NULL DEFAULT (datetime('now')),
                created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create projects table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS documents (
                project_id  INTEGER NOT NULL REFERENCES projects(id),
                doc_type    TEXT    NOT NULL,
                content     TEXT    NOT NULL DEFAULT '',
                updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (project_id, doc_type)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create documents table");

        pool
    }

    #[tokio::test]
    async fn upsert_project_creates_new_project() {
        let pool = temp_db().await;

        let project = upsert_project(&pool, "/home/user/my-project")
            .await
            .expect("Failed to upsert project");

        assert_eq!(project.folder_path, "/home/user/my-project");
        assert_eq!(project.name, "my-project");
        assert!(project.id > 0);
    }

    #[tokio::test]
    async fn upsert_project_reopens_closed_project() {
        let pool = temp_db().await;

        // Create project
        let project1 = upsert_project(&pool, "/home/user/my-project")
            .await
            .expect("Failed to create project");

        // Close it
        close_project(&pool, project1.id)
            .await
            .expect("Failed to close project");

        // Verify it's closed
        let open_projects = list_open_projects(&pool)
            .await
            .expect("Failed to list projects");
        assert_eq!(open_projects.len(), 0);

        // Reopen by upserting again
        let project2 = upsert_project(&pool, "/home/user/my-project")
            .await
            .expect("Failed to reopen project");

        assert_eq!(project1.id, project2.id);
        assert_eq!(project2.folder_path, "/home/user/my-project");

        // Verify it's open now
        let open_projects = list_open_projects(&pool)
            .await
            .expect("Failed to list projects");
        assert_eq!(open_projects.len(), 1);
    }

    #[tokio::test]
    async fn list_open_projects_returns_only_open() {
        let pool = temp_db().await;

        // Create three projects
        let p1 = upsert_project(&pool, "/home/user/project-a")
            .await
            .expect("Failed to create project-a");
        let _p2 = upsert_project(&pool, "/home/user/project-b")
            .await
            .expect("Failed to create project-b");
        let _p3 = upsert_project(&pool, "/home/user/project-c")
            .await
            .expect("Failed to create project-c");

        // Close one
        close_project(&pool, p1.id)
            .await
            .expect("Failed to close project");

        // Should only return 2 open projects
        let open_projects = list_open_projects(&pool)
            .await
            .expect("Failed to list projects");
        assert_eq!(open_projects.len(), 2);

        let names: Vec<String> = open_projects.iter().map(|p| p.name.clone()).collect();
        assert!(names.contains(&"project-b".to_string()));
        assert!(names.contains(&"project-c".to_string()));
        assert!(!names.contains(&"project-a".to_string()));
    }

    #[tokio::test]
    async fn list_open_projects_ordered_by_most_recent() {
        let pool = temp_db().await;

        // Create projects in order
        let _p1 = upsert_project(&pool, "/home/user/old-project")
            .await
            .expect("Failed to create old-project");

        // Longer delay to ensure different timestamps (SQLite datetime has second precision)
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let _p2 = upsert_project(&pool, "/home/user/newer-project")
            .await
            .expect("Failed to create newer-project");

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let _p3 = upsert_project(&pool, "/home/user/newest-project")
            .await
            .expect("Failed to create newest-project");

        let open_projects = list_open_projects(&pool)
            .await
            .expect("Failed to list projects");

        // Should be in reverse chronological order (newest first)
        assert_eq!(open_projects[0].name, "newest-project");
        assert_eq!(open_projects[1].name, "newer-project");
        assert_eq!(open_projects[2].name, "old-project");
    }

    #[tokio::test]
    async fn close_project_sets_is_open_to_zero() {
        let pool = temp_db().await;

        let project = upsert_project(&pool, "/home/user/my-project")
            .await
            .expect("Failed to create project");

        close_project(&pool, project.id)
            .await
            .expect("Failed to close project");

        let open_projects = list_open_projects(&pool)
            .await
            .expect("Failed to list projects");

        assert_eq!(open_projects.len(), 0);
    }

    #[tokio::test]
    async fn upsert_document_stores_content() {
        let pool = temp_db().await;

        let project = upsert_project(&pool, "/home/user/my-project")
            .await
            .expect("Failed to create project");

        upsert_document(&pool, project.id, "prd", "# Product Requirements\n\nGoal: Build a thing")
            .await
            .expect("Failed to upsert document");

        let docs = get_documents(&pool, project.id)
            .await
            .expect("Failed to get documents");

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].doc_type, "prd");
        assert_eq!(docs[0].content, "# Product Requirements\n\nGoal: Build a thing");
    }

    #[tokio::test]
    async fn upsert_document_updates_existing() {
        let pool = temp_db().await;

        let project = upsert_project(&pool, "/home/user/my-project")
            .await
            .expect("Failed to create project");

        // Insert initial content
        upsert_document(&pool, project.id, "prd", "Version 1")
            .await
            .expect("Failed to upsert document");

        // Update with new content
        upsert_document(&pool, project.id, "prd", "Version 2")
            .await
            .expect("Failed to update document");

        let docs = get_documents(&pool, project.id)
            .await
            .expect("Failed to get documents");

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].content, "Version 2");
    }

    #[tokio::test]
    async fn get_documents_returns_multiple_doc_types() {
        let pool = temp_db().await;

        let project = upsert_project(&pool, "/home/user/my-project")
            .await
            .expect("Failed to create project");

        upsert_document(&pool, project.id, "prd", "PRD content")
            .await
            .expect("Failed to upsert PRD");

        upsert_document(&pool, project.id, "spec", "Spec content")
            .await
            .expect("Failed to upsert spec");

        let docs = get_documents(&pool, project.id)
            .await
            .expect("Failed to get documents");

        assert_eq!(docs.len(), 2);

        let doc_types: Vec<String> = docs.iter().map(|d| d.doc_type.clone()).collect();
        assert!(doc_types.contains(&"prd".to_string()));
        assert!(doc_types.contains(&"spec".to_string()));
    }

    #[tokio::test]
    async fn get_documents_returns_empty_for_project_without_docs() {
        let pool = temp_db().await;

        let project = upsert_project(&pool, "/home/user/my-project")
            .await
            .expect("Failed to create project");

        let docs = get_documents(&pool, project.id)
            .await
            .expect("Failed to get documents");

        assert_eq!(docs.len(), 0);
    }

    #[tokio::test]
    async fn project_name_extracted_from_folder_path() {
        let pool = temp_db().await;

        // Only test Unix-style paths (this code runs on macOS/Linux servers)
        // Windows paths would be handled by Windows servers
        let test_cases = vec![
            ("/home/user/my-project", "my-project"),
            ("/var/www/site", "site"),
            ("/single", "single"),
            ("/path/with-dashes", "with-dashes"),
        ];

        for (path, expected_name) in test_cases {
            let project = upsert_project(&pool, path)
                .await
                .expect("Failed to create project");
            assert_eq!(project.name, expected_name, "Failed for path: {}", path);
        }
    }

    #[tokio::test]
    async fn upsert_project_is_idempotent() {
        let pool = temp_db().await;

        // Call upsert multiple times with same path
        let p1 = upsert_project(&pool, "/home/user/same-project")
            .await
            .expect("Failed first upsert");

        let p2 = upsert_project(&pool, "/home/user/same-project")
            .await
            .expect("Failed second upsert");

        let p3 = upsert_project(&pool, "/home/user/same-project")
            .await
            .expect("Failed third upsert");

        // All should have the same ID
        assert_eq!(p1.id, p2.id);
        assert_eq!(p2.id, p3.id);

        // Should only have one project in the database
        let all_projects = list_open_projects(&pool)
            .await
            .expect("Failed to list projects");
        assert_eq!(all_projects.len(), 1);
    }
}
