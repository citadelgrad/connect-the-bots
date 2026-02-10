use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

/// Result of executing a shell command.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
    pub duration_ms: u64,
}

/// Entry returned by directory listing.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

/// Options for grep operations.
#[derive(Debug, Clone, Default)]
pub struct GrepOptions {
    pub case_insensitive: bool,
    pub max_results: Option<usize>,
    pub include_line_numbers: bool,
    pub context_lines: usize,
}

/// Abstraction over the execution environment that tools operate in.
#[async_trait]
pub trait ExecutionEnvironment: Send + Sync {
    async fn read_file(&self, path: &Path) -> attractor_types::Result<String>;
    async fn write_file(&self, path: &Path, content: &str) -> attractor_types::Result<()>;
    async fn file_exists(&self, path: &Path) -> attractor_types::Result<bool>;
    async fn list_directory(
        &self,
        path: &Path,
        depth: usize,
    ) -> attractor_types::Result<Vec<DirEntry>>;
    async fn exec_command(
        &self,
        command: &str,
        timeout_ms: u64,
        cwd: Option<&Path>,
        env_vars: Option<&HashMap<String, String>>,
    ) -> attractor_types::Result<ExecResult>;
    async fn grep(
        &self,
        pattern: &str,
        path: &Path,
        options: &GrepOptions,
    ) -> attractor_types::Result<String>;
    async fn glob_files(&self, pattern: &str, base: &Path)
        -> attractor_types::Result<Vec<PathBuf>>;
    fn working_directory(&self) -> &Path;
    fn platform(&self) -> &str;
}
