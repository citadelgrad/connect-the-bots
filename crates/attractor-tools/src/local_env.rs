use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use globset::{Glob, GlobSetBuilder};
use regex::RegexBuilder;

use crate::environment::{DirEntry, ExecResult, ExecutionEnvironment, GrepOptions};

/// Concrete execution environment that runs on the local filesystem.
pub struct LocalExecutionEnvironment {
    working_dir: PathBuf,
    platform: String,
}

impl LocalExecutionEnvironment {
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self {
            working_dir: working_dir.into(),
            platform: std::env::consts::OS.to_string(),
        }
    }

    pub fn current_dir() -> std::io::Result<Self> {
        Ok(Self::new(std::env::current_dir()?))
    }

    /// Resolve a path relative to the working directory if it is not absolute.
    fn resolve(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }

    /// Filter environment variables, removing secrets and keeping safe defaults.
    fn filtered_env() -> HashMap<String, String> {
        let exclude_suffixes: &[&str] = &[
            "_api_key",
            "_secret",
            "_token",
            "_password",
            "_credential",
        ];
        let always_include: &[&str] = &[
            "PATH", "HOME", "USER", "SHELL", "LANG", "TERM", "TMPDIR",
        ];

        let mut result = HashMap::new();
        for (key, value) in std::env::vars() {
            let lower = key.to_lowercase();
            if always_include.contains(&key.as_str()) {
                result.insert(key, value);
            } else if !exclude_suffixes.iter().any(|s| lower.ends_with(s)) {
                result.insert(key, value);
            }
        }
        result
    }
}

#[async_trait]
impl ExecutionEnvironment for LocalExecutionEnvironment {
    async fn read_file(&self, path: &Path) -> attractor_types::Result<String> {
        let resolved = self.resolve(path);
        Ok(tokio::fs::read_to_string(&resolved).await?)
    }

    async fn write_file(&self, path: &Path, content: &str) -> attractor_types::Result<()> {
        let resolved = self.resolve(path);
        if let Some(parent) = resolved.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        Ok(tokio::fs::write(&resolved, content).await?)
    }

    async fn file_exists(&self, path: &Path) -> attractor_types::Result<bool> {
        let resolved = self.resolve(path);
        Ok(tokio::fs::try_exists(&resolved).await?)
    }

    async fn list_directory(
        &self,
        path: &Path,
        depth: usize,
    ) -> attractor_types::Result<Vec<DirEntry>> {
        let resolved = self.resolve(path);
        let mut entries = Vec::new();
        list_dir_recursive(&resolved, depth, &mut entries).await?;
        Ok(entries)
    }

    async fn exec_command(
        &self,
        command: &str,
        timeout_ms: u64,
        cwd: Option<&Path>,
        env_vars: Option<&HashMap<String, String>>,
    ) -> attractor_types::Result<ExecResult> {
        let work_dir = cwd.map(|p| self.resolve(p)).unwrap_or_else(|| self.working_dir.clone());

        let mut cmd = tokio::process::Command::new("bash");
        cmd.args(["-c", command])
            .current_dir(&work_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .envs(Self::filtered_env());

        // On Unix, use process_group for clean kill
        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        if let Some(vars) = env_vars {
            cmd.envs(vars);
        }

        let start = tokio::time::Instant::now();
        let mut child = cmd.spawn()?;

        // Take stdout/stderr handles so we can read them while retaining child ownership
        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut stderr = child.stderr.take().expect("stderr piped");

        let timeout_dur = std::time::Duration::from_millis(timeout_ms);

        tokio::select! {
            status = child.wait() => {
                let status = status?;
                // Read remaining output
                let mut stdout_buf = Vec::new();
                let mut stderr_buf = Vec::new();
                use tokio::io::AsyncReadExt;
                let _ = stdout.read_to_end(&mut stdout_buf).await;
                let _ = stderr.read_to_end(&mut stderr_buf).await;

                let duration_ms = start.elapsed().as_millis() as u64;
                Ok(ExecResult {
                    stdout: String::from_utf8_lossy(&stdout_buf).to_string(),
                    stderr: String::from_utf8_lossy(&stderr_buf).to_string(),
                    exit_code: status.code().unwrap_or(-1),
                    timed_out: false,
                    duration_ms,
                })
            }
            _ = tokio::time::sleep(timeout_dur) => {
                // Timeout: try graceful termination first
                #[cfg(unix)]
                {
                    if let Some(pid) = child.id() {
                        // Send SIGTERM to the process group
                        unsafe { libc::kill(-(pid as i32), libc::SIGTERM); }
                    }
                    // Wait 2 seconds for graceful shutdown
                    tokio::select! {
                        _ = child.wait() => {}
                        _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                            // Force kill
                            let _ = child.kill().await;
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = child.kill().await;
                }

                let duration_ms = start.elapsed().as_millis() as u64;
                Ok(ExecResult {
                    stdout: String::new(),
                    stderr: format!("Command timed out after {}ms", timeout_ms),
                    exit_code: -1,
                    timed_out: true,
                    duration_ms,
                })
            }
        }
    }

    async fn grep(
        &self,
        pattern: &str,
        path: &Path,
        options: &GrepOptions,
    ) -> attractor_types::Result<String> {
        let resolved = self.resolve(path);

        // Try ripgrep first
        let rg_result = try_ripgrep(pattern, &resolved, options).await;
        if let Ok(output) = rg_result {
            return Ok(output);
        }

        // Fallback: use regex crate
        grep_with_regex(pattern, &resolved, options).await
    }

    async fn glob_files(
        &self,
        pattern: &str,
        base: &Path,
    ) -> attractor_types::Result<Vec<PathBuf>> {
        let resolved = self.resolve(base);
        let glob = Glob::new(pattern).map_err(|e| {
            attractor_types::AttractorError::ToolError {
                tool: "glob".into(),
                message: e.to_string(),
            }
        })?;
        let mut builder = GlobSetBuilder::new();
        builder.add(glob);
        let set = builder.build().map_err(|e| {
            attractor_types::AttractorError::ToolError {
                tool: "glob".into(),
                message: e.to_string(),
            }
        })?;

        let mut matches = Vec::new();
        collect_glob_matches(&resolved, &resolved, &set, &mut matches).await?;
        matches.sort();
        Ok(matches)
    }

    fn working_directory(&self) -> &Path {
        &self.working_dir
    }

    fn platform(&self) -> &str {
        &self.platform
    }
}

/// Recursively list directory entries up to a given depth.
async fn list_dir_recursive(
    path: &Path,
    depth: usize,
    entries: &mut Vec<DirEntry>,
) -> attractor_types::Result<()> {
    let mut read_dir = tokio::fs::read_dir(path).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let meta = entry.metadata().await?;
        let is_dir = meta.is_dir();
        entries.push(DirEntry {
            path: entry.path(),
            is_dir,
            size: meta.len(),
        });
        if is_dir && depth > 1 {
            Box::pin(list_dir_recursive(&entry.path(), depth - 1, entries)).await?;
        }
    }
    Ok(())
}

/// Try running ripgrep as a subprocess.
async fn try_ripgrep(
    pattern: &str,
    path: &Path,
    options: &GrepOptions,
) -> std::result::Result<String, ()> {
    let mut args = vec!["--no-heading".to_string()];
    if options.case_insensitive {
        args.push("-i".to_string());
    }
    if options.include_line_numbers {
        args.push("-n".to_string());
    }
    if options.context_lines > 0 {
        args.push(format!("-C{}", options.context_lines));
    }
    if let Some(max) = options.max_results {
        args.push(format!("-m{}", max));
    }
    args.push(pattern.to_string());
    args.push(path.to_string_lossy().to_string());

    let output = tokio::process::Command::new("rg")
        .args(&args)
        .output()
        .await
        .map_err(|_| ())?;

    // rg exits 0 on match, 1 on no match, 2 on error
    if output.status.code() == Some(2) {
        return Err(());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Fallback grep using the regex crate (searches files recursively).
async fn grep_with_regex(
    pattern: &str,
    path: &Path,
    options: &GrepOptions,
) -> attractor_types::Result<String> {
    let re = RegexBuilder::new(pattern)
        .case_insensitive(options.case_insensitive)
        .build()
        .map_err(|e| attractor_types::AttractorError::ToolError {
            tool: "grep".into(),
            message: e.to_string(),
        })?;

    let mut results = Vec::new();
    let max = options.max_results.unwrap_or(usize::MAX);

    grep_path_recursive(&re, path, options, &mut results, max).await?;
    Ok(results.join("\n"))
}

/// Recursively search files for regex matches.
async fn grep_path_recursive(
    re: &regex::Regex,
    path: &Path,
    options: &GrepOptions,
    results: &mut Vec<String>,
    max: usize,
) -> attractor_types::Result<()> {
    if results.len() >= max {
        return Ok(());
    }

    let meta = tokio::fs::metadata(path).await?;
    if meta.is_file() {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            for (i, line) in content.lines().enumerate() {
                if results.len() >= max {
                    break;
                }
                if re.is_match(line) {
                    if options.include_line_numbers {
                        results.push(format!("{}:{}:{}", path.display(), i + 1, line));
                    } else {
                        results.push(format!("{}:{}", path.display(), line));
                    }
                }
            }
        }
    } else if meta.is_dir() {
        let mut read_dir = tokio::fs::read_dir(path).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            if results.len() >= max {
                break;
            }
            Box::pin(grep_path_recursive(re, &entry.path(), options, results, max)).await?;
        }
    }
    Ok(())
}

/// Recursively collect files matching a globset.
async fn collect_glob_matches(
    base: &Path,
    current: &Path,
    set: &globset::GlobSet,
    matches: &mut Vec<PathBuf>,
) -> attractor_types::Result<()> {
    let meta = tokio::fs::metadata(current).await?;
    if meta.is_file() {
        if let Ok(rel) = current.strip_prefix(base) {
            if set.is_match(rel) {
                matches.push(current.to_path_buf());
            }
        }
    } else if meta.is_dir() {
        let mut read_dir = tokio::fs::read_dir(current).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            Box::pin(collect_glob_matches(base, &entry.path(), set, matches)).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_env(dir: &TempDir) -> LocalExecutionEnvironment {
        LocalExecutionEnvironment::new(dir.path())
    }

    #[tokio::test]
    async fn read_write_round_trip() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);

        let path = Path::new("test.txt");
        env.write_file(path, "hello world").await.unwrap();
        let content = env.read_file(path).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn write_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);

        let path = Path::new("sub/dir/file.txt");
        env.write_file(path, "nested").await.unwrap();
        let content = env.read_file(path).await.unwrap();
        assert_eq!(content, "nested");
    }

    #[tokio::test]
    async fn file_exists_true_and_false() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);

        assert!(!env.file_exists(Path::new("nope.txt")).await.unwrap());

        env.write_file(Path::new("yes.txt"), "data").await.unwrap();
        assert!(env.file_exists(Path::new("yes.txt")).await.unwrap());
    }

    #[tokio::test]
    async fn exec_command_echo() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);

        let result = env
            .exec_command("echo hello", 5000, None, None)
            .await
            .unwrap();
        assert_eq!(result.stdout.trim(), "hello");
        assert_eq!(result.exit_code, 0);
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn exec_command_timeout() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);

        let result = env
            .exec_command("sleep 60", 100, None, None)
            .await
            .unwrap();
        assert!(result.timed_out);
        assert!(result.duration_ms >= 100);
    }

    #[tokio::test]
    async fn platform_returns_os() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);
        assert_eq!(env.platform(), std::env::consts::OS);
    }

    #[tokio::test]
    async fn glob_files_finds_matching() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);

        env.write_file(Path::new("a.rs"), "rust").await.unwrap();
        env.write_file(Path::new("b.rs"), "rust").await.unwrap();
        env.write_file(Path::new("c.txt"), "text").await.unwrap();

        let matches = env.glob_files("*.rs", Path::new("")).await.unwrap();
        assert_eq!(matches.len(), 2);
        for m in &matches {
            assert!(m.to_string_lossy().ends_with(".rs"));
        }
    }

    #[tokio::test]
    async fn list_directory_basic() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);

        env.write_file(Path::new("file1.txt"), "a").await.unwrap();
        env.write_file(Path::new("file2.txt"), "b").await.unwrap();

        let entries = env.list_directory(Path::new(""), 1).await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn working_directory_matches() {
        let dir = TempDir::new().unwrap();
        let env = make_env(&dir);
        assert_eq!(env.working_directory(), dir.path());
    }
}
