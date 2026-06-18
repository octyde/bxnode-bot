//! Coding tools for workspace-scoped file operations, search, and shell execution.
//!
//! All tools operate within a designated project workspace directory.
//! Path sandboxing ensures no operations can escape the workspace.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::json;

use super::tools::Tool;

/// Maximum file size to read (512 KB)
const MAX_READ_SIZE: u64 = 512 * 1024;

/// Maximum output size for shell commands (128 KB)
const MAX_SHELL_OUTPUT: usize = 128 * 1024;

/// Default shell timeout in seconds
const DEFAULT_SHELL_TIMEOUT: u64 = 30;

/// Maximum shell timeout in seconds
const MAX_SHELL_TIMEOUT: u64 = 120;

/// Maximum directory listing depth
const MAX_LIST_DEPTH: u32 = 5;

/// Maximum number of search results
const MAX_SEARCH_RESULTS: usize = 50;

/// Check if an environment variable key contains sensitive data (API keys, tokens, secrets).
/// Used by ShellExecTool to sanitize the environment before running commands.
fn is_sensitive_env_key(key: &str) -> bool {
    let upper = key.to_uppercase();
    upper.ends_with("_API_KEY")
        || upper.ends_with("_APIKEY")
        || upper.ends_with("_TOKEN")
        || upper.ends_with("_SECRET")
        || upper.ends_with("_PASSWORD")
        || upper.ends_with("_CREDENTIALS")
        || upper == "ANTHROPIC_API_KEY"
        || upper == "OPENAI_API_KEY"
        || upper == "GEMINI_API_KEY"
        || upper == "BRAVE_API_KEY"
        || upper == "PERPLEXITY_API_KEY"
}

// ============================================================================
// Path sandboxing
// ============================================================================

/// Resolve and validate a path within the workspace.
/// Returns an error if the resolved path escapes the workspace.
fn resolve_workspace_path(workspace: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    // Reject absolute paths
    if relative.starts_with('/') || relative.starts_with('\\') {
        anyhow::bail!("Absolute paths are not allowed. Use paths relative to the workspace.");
    }

    let candidate = workspace.join(relative);

    // Try to canonicalize. For existing paths this resolves symlinks.
    // For non-existing paths (new files), canonicalize the parent.
    let resolved = if candidate.exists() {
        candidate.canonicalize()?
    } else {
        let parent = candidate
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid path: no parent directory"))?;
        if !parent.exists() {
            // Parent doesn't exist either — check grandparent etc.
            // We'll just do a simple component-based check instead
            let workspace_canonical = workspace.canonicalize()?;
            let normalized = normalize_path(&candidate);
            if !normalized.starts_with(&workspace_canonical) {
                anyhow::bail!("Path escapes workspace: {}", relative);
            }
            return Ok(normalized);
        }
        let parent_canonical = parent.canonicalize()?;
        parent_canonical.join(candidate.file_name().unwrap_or_default())
    };

    let workspace_canonical = workspace.canonicalize()?;
    if !resolved.starts_with(&workspace_canonical) {
        anyhow::bail!("Path escapes workspace: {}", relative);
    }

    Ok(resolved)
}

/// Simple path normalization without filesystem access
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            _ => result.push(component),
        }
    }
    result
}

// ============================================================================
// file_read
// ============================================================================

pub struct FileReadTool {
    workspace: PathBuf,
}

impl FileReadTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn metadata(&self) -> super::tool_policy::ToolMetadata {
        super::tool_policy::ToolMetadata::read_only()
    }

    fn description(&self) -> &str {
        "Read the contents of a file in the project workspace. Supports optional line range."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path to the file within the workspace"
                },
                "line_start": {
                    "type": "integer",
                    "description": "Start line number (1-based, inclusive). Omit to read from beginning."
                },
                "line_end": {
                    "type": "integer",
                    "description": "End line number (1-based, inclusive). Omit to read to end."
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let rel_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let path = resolve_workspace_path(&self.workspace, rel_path)?;

        if !path.exists() {
            anyhow::bail!("File not found: {}", rel_path);
        }
        if !path.is_file() {
            anyhow::bail!("Not a file: {}", rel_path);
        }

        // Check file size
        let metadata = tokio::fs::metadata(&path).await?;
        if metadata.len() > MAX_READ_SIZE {
            anyhow::bail!(
                "File too large ({} bytes, max {} bytes). Use line_start/line_end to read a range.",
                metadata.len(),
                MAX_READ_SIZE
            );
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let lines: Vec<&str> = content.lines().collect();

        let line_start = input
            .get("line_start")
            .and_then(|v| v.as_u64())
            .map(|v| v.max(1) as usize)
            .unwrap_or(1);
        let line_end = input
            .get("line_end")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(lines.len());

        let start_idx = (line_start - 1).min(lines.len());
        let end_idx = line_end.min(lines.len());

        let mut output = String::new();
        for (i, line) in lines[start_idx..end_idx].iter().enumerate() {
            output.push_str(&format!("{:>4} | {}\n", start_idx + i + 1, line));
        }

        if output.is_empty() {
            output = "(empty file)".to_string();
        }

        Ok(output)
    }
}

// ============================================================================
// file_write
// ============================================================================

pub struct FileWriteTool {
    workspace: PathBuf,
}

impl FileWriteTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write content to a file in the project workspace. Creates the file if it doesn't exist. Overwrites existing content."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path to the file within the workspace"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                },
                "create_dirs": {
                    "type": "boolean",
                    "description": "Create intermediate directories if they don't exist (default: true)",
                    "default": true
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let rel_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;
        let create_dirs = input
            .get("create_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let path = resolve_workspace_path(&self.workspace, rel_path)?;

        if create_dirs {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        tokio::fs::write(&path, content).await?;

        Ok(format!("Written {} bytes to {}", content.len(), rel_path))
    }
}

// ============================================================================
// file_edit
// ============================================================================

pub struct FileEditTool {
    workspace: PathBuf,
}

impl FileEditTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Apply a targeted search/replace edit to a file. The old_text must match exactly."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path to the file within the workspace"
                },
                "old_text": {
                    "type": "string",
                    "description": "The exact text to find and replace"
                },
                "new_text": {
                    "type": "string",
                    "description": "The replacement text"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let rel_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let old_text = input
            .get("old_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'old_text' parameter"))?;
        let new_text = input
            .get("new_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_text' parameter"))?;

        let path = resolve_workspace_path(&self.workspace, rel_path)?;

        if !path.exists() {
            anyhow::bail!("File not found: {}", rel_path);
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let count = content.matches(old_text).count();

        if count == 0 {
            anyhow::bail!("old_text not found in file. Make sure it matches exactly (including whitespace).");
        }

        let new_content = content.replacen(old_text, new_text, 1);
        tokio::fs::write(&path, &new_content).await?;

        Ok(format!(
            "Replaced 1 occurrence in {} ({} total matches found)",
            rel_path, count
        ))
    }
}

// ============================================================================
// file_delete
// ============================================================================

pub struct FileDeleteTool {
    workspace: PathBuf,
}

impl FileDeleteTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FileDeleteTool {
    fn name(&self) -> &str {
        "file_delete"
    }

    fn metadata(&self) -> super::tool_policy::ToolMetadata {
        super::tool_policy::ToolMetadata::destructive()
    }

    fn description(&self) -> &str {
        "Delete a file from the project workspace."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path to the file within the workspace"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let rel_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let path = resolve_workspace_path(&self.workspace, rel_path)?;

        if !path.exists() {
            anyhow::bail!("File not found: {}", rel_path);
        }
        if !path.is_file() {
            anyhow::bail!("Not a file (use shell for directory removal): {}", rel_path);
        }

        tokio::fs::remove_file(&path).await?;

        Ok(format!("Deleted {}", rel_path))
    }
}

// ============================================================================
// list_directory
// ============================================================================

pub struct ListDirectoryTool {
    workspace: PathBuf,
}

impl ListDirectoryTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn metadata(&self) -> super::tool_policy::ToolMetadata {
        super::tool_policy::ToolMetadata::read_only()
    }

    fn description(&self) -> &str {
        "List the contents of a directory in the project workspace."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path to the directory (default: workspace root)",
                    "default": "."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List recursively (default: false)",
                    "default": false
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum depth for recursive listing (default: 3)",
                    "default": 3
                }
            }
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let rel_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let recursive = input
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_depth = input
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(3)
            .min(MAX_LIST_DEPTH as u64) as u32;

        let path = resolve_workspace_path(&self.workspace, rel_path)?;

        if !path.exists() {
            anyhow::bail!("Directory not found: {}", rel_path);
        }
        if !path.is_dir() {
            anyhow::bail!("Not a directory: {}", rel_path);
        }

        let mut output = String::new();
        let mut count = 0;
        list_dir_recursive(&path, &self.workspace, &mut output, 0, max_depth, recursive, &mut count).await?;

        if count == 0 {
            output = "(empty directory)".to_string();
        } else {
            output.push_str(&format!("\n({} entries)", count));
        }

        Ok(output)
    }
}

/// Recursively list directory contents
#[async_recursion::async_recursion]
async fn list_dir_recursive(
    dir: &Path,
    workspace: &Path,
    output: &mut String,
    depth: u32,
    max_depth: u32,
    recursive: bool,
    count: &mut usize,
) -> anyhow::Result<()> {
    let mut entries = tokio::fs::read_dir(dir).await?;
    let mut items: Vec<(String, bool)> = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden files at top level to reduce noise
        if name.starts_with('.') && depth == 0 {
            continue;
        }
        let is_dir = entry.file_type().await?.is_dir();
        items.push((name, is_dir));
    }

    items.sort_by(|a, b| {
        // Directories first, then alphabetical
        b.1.cmp(&a.1).then(a.0.cmp(&b.0))
    });

    let indent = "  ".repeat(depth as usize);
    for (name, is_dir) in &items {
        *count += 1;
        if *is_dir {
            output.push_str(&format!("{}{}/\n", indent, name));
            if recursive && depth < max_depth {
                let child = dir.join(name);
                list_dir_recursive(&child, workspace, output, depth + 1, max_depth, recursive, count).await?;
            }
        } else {
            output.push_str(&format!("{}{}\n", indent, name));
        }
    }

    Ok(())
}

// ============================================================================
// file_search
// ============================================================================

pub struct FileSearchTool {
    workspace: PathBuf,
}

impl FileSearchTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FileSearchTool {
    fn name(&self) -> &str {
        "file_search"
    }

    fn metadata(&self) -> super::tool_policy::ToolMetadata {
        super::tool_policy::ToolMetadata::read_only()
    }

    fn description(&self) -> &str {
        "Search for a pattern in files within the project workspace. Uses regex matching."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Relative directory to search in (default: workspace root)",
                    "default": "."
                },
                "glob": {
                    "type": "string",
                    "description": "File glob pattern to filter (e.g., '*.rs', '*.py')"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 20)",
                    "default": 20
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let pattern_str = input
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter"))?;
        let rel_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let glob_pattern = input.get("glob").and_then(|v| v.as_str());
        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(MAX_SEARCH_RESULTS as u64) as usize;

        let search_dir = resolve_workspace_path(&self.workspace, rel_path)?;
        let regex = regex::Regex::new(pattern_str)
            .map_err(|e| anyhow::anyhow!("Invalid regex: {}", e))?;

        let glob_matcher = glob_pattern.map(|g| glob::Pattern::new(g)).transpose()
            .map_err(|e| anyhow::anyhow!("Invalid glob: {}", e))?;

        let mut results = Vec::new();
        search_files_recursive(&search_dir, &self.workspace, &regex, &glob_matcher, &mut results, max_results).await?;

        if results.is_empty() {
            return Ok(format!("No matches found for pattern '{}'", pattern_str));
        }

        let mut output = String::new();
        for (file_rel, line_num, line_content) in &results {
            output.push_str(&format!("{}:{}: {}\n", file_rel, line_num, line_content));
        }
        output.push_str(&format!("\n({} matches)", results.len()));

        Ok(output)
    }
}

/// Recursively search files for a pattern
#[async_recursion::async_recursion]
async fn search_files_recursive(
    dir: &Path,
    workspace: &Path,
    regex: &regex::Regex,
    glob_matcher: &Option<glob::Pattern>,
    results: &mut Vec<(String, usize, String)>,
    max_results: usize,
) -> anyhow::Result<()> {
    if results.len() >= max_results {
        return Ok(());
    }

    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    while let Some(entry) = entries.next_entry().await? {
        if results.len() >= max_results {
            break;
        }

        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden directories and common non-code directories
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
            continue;
        }

        if path.is_dir() {
            search_files_recursive(&path, workspace, regex, glob_matcher, results, max_results).await?;
        } else if path.is_file() {
            // Check glob filter
            if let Some(ref matcher) = glob_matcher {
                if !matcher.matches(&name) {
                    continue;
                }
            }

            // Skip binary files (check first few bytes)
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let rel_path = path
                    .strip_prefix(workspace)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                for (i, line) in content.lines().enumerate() {
                    if results.len() >= max_results {
                        break;
                    }
                    if regex.is_match(line) {
                        results.push((rel_path.clone(), i + 1, line.trim().to_string()));
                    }
                }
            }
        }
    }

    Ok(())
}

// ============================================================================
// shell_exec
// ============================================================================

pub struct ShellExecTool {
    workspace: PathBuf,
}

impl ShellExecTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for ShellExecTool {
    fn name(&self) -> &str {
        "shell_exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the project workspace directory. Use for running tests, builds, git operations, etc."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 120)",
                    "default": 30
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?;
        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_SHELL_TIMEOUT)
            .min(MAX_SHELL_TIMEOUT);

        // Security: build a sanitized environment by stripping secrets
        let filtered_env: std::collections::HashMap<String, String> = std::env::vars()
            .filter(|(key, _)| !is_sensitive_env_key(key))
            .collect();

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&self.workspace)
                .env_clear()
                .envs(&filtered_env)
                .output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out after {}s", timeout_secs))?
        .map_err(|e| anyhow::anyhow!("Failed to execute command: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut result = String::new();
        result.push_str(&format!("Exit code: {}\n", exit_code));

        if !stdout.is_empty() {
            let stdout_str = if stdout.len() > MAX_SHELL_OUTPUT {
                format!(
                    "{}... (truncated, {} bytes total)",
                    &stdout[..MAX_SHELL_OUTPUT],
                    stdout.len()
                )
            } else {
                stdout.to_string()
            };
            result.push_str(&format!("\nSTDOUT:\n{}", stdout_str));
        }

        if !stderr.is_empty() {
            let stderr_str = if stderr.len() > MAX_SHELL_OUTPUT {
                format!(
                    "{}... (truncated, {} bytes total)",
                    &stderr[..MAX_SHELL_OUTPUT],
                    stderr.len()
                )
            } else {
                stderr.to_string()
            };
            result.push_str(&format!("\nSTDERR:\n{}", stderr_str));
        }

        Ok(result)
    }
}

// ============================================================================
// git_status
// ============================================================================

pub struct GitStatusTool {
    workspace: PathBuf,
}

impl GitStatusTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn metadata(&self) -> super::tool_policy::ToolMetadata {
        super::tool_policy::ToolMetadata::read_only()
    }

    fn description(&self) -> &str {
        "Show git status and recent commit log for the project workspace."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _input: serde_json::Value) -> anyhow::Result<String> {
        let mut result = String::new();

        // Check if it's a git repo
        let git_dir = self.workspace.join(".git");
        if !git_dir.exists() {
            return Ok("Not a git repository. Use shell_exec to run 'git init' to initialize one.".to_string());
        }

        // git status
        let status_output = tokio::process::Command::new("git")
            .args(["status", "--short"])
            .current_dir(&self.workspace)
            .output()
            .await?;

        let status = String::from_utf8_lossy(&status_output.stdout);
        result.push_str("Git Status:\n");
        if status.trim().is_empty() {
            result.push_str("  (clean working tree)\n");
        } else {
            result.push_str(&status);
        }

        // git log
        let log_output = tokio::process::Command::new("git")
            .args(["log", "--oneline", "-5"])
            .current_dir(&self.workspace)
            .output()
            .await?;

        let log = String::from_utf8_lossy(&log_output.stdout);
        if !log.trim().is_empty() {
            result.push_str("\nRecent Commits:\n");
            result.push_str(&log);
        }

        // Current branch
        let branch_output = tokio::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&self.workspace)
            .output()
            .await?;

        let branch = String::from_utf8_lossy(&branch_output.stdout);
        if !branch.trim().is_empty() {
            result.push_str(&format!("\nBranch: {}", branch.trim()));
        }

        Ok(result)
    }
}

// ============================================================================
// git_diff
// ============================================================================

pub struct GitDiffTool {
    workspace: PathBuf,
}

impl GitDiffTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn metadata(&self) -> super::tool_policy::ToolMetadata {
        super::tool_policy::ToolMetadata::read_only()
    }

    fn description(&self) -> &str {
        "Show git diff for the project workspace. Can show staged, unstaged, or diff against a specific commit."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "What to diff: 'staged', 'unstaged' (default), or a commit ref (e.g., 'HEAD~3', 'main')",
                    "default": "unstaged"
                },
                "path": {
                    "type": "string",
                    "description": "Optional file path to filter the diff"
                }
            }
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let target = input
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unstaged");
        let path_filter = input.get("path").and_then(|v| v.as_str());

        let mut args = vec!["diff".to_string()];

        match target {
            "staged" => args.push("--staged".to_string()),
            "unstaged" => {} // default git diff
            ref_str => args.push(ref_str.to_string()),
        }

        if let Some(p) = path_filter {
            args.push("--".to_string());
            args.push(p.to_string());
        }

        let output = tokio::process::Command::new("git")
            .args(&args)
            .current_dir(&self.workspace)
            .output()
            .await?;

        let diff = String::from_utf8_lossy(&output.stdout);
        if diff.trim().is_empty() {
            return Ok("No differences found.".to_string());
        }

        // Truncate large diffs
        let result = if diff.len() > MAX_SHELL_OUTPUT {
            format!(
                "{}... (truncated, {} bytes total)",
                &diff[..MAX_SHELL_OUTPUT],
                diff.len()
            )
        } else {
            diff.to_string()
        };

        Ok(result)
    }
}

// ============================================================================
// git_log
// ============================================================================

pub struct GitLogTool {
    workspace: PathBuf,
}

impl GitLogTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for GitLogTool {
    fn name(&self) -> &str {
        "git_log"
    }

    fn metadata(&self) -> super::tool_policy::ToolMetadata {
        super::tool_policy::ToolMetadata::read_only()
    }

    fn description(&self) -> &str {
        "Show git commit log for the project workspace with optional filtering."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer",
                    "description": "Number of commits to show (default: 10, max: 50)",
                    "default": 10,
                    "maximum": 50
                },
                "path": {
                    "type": "string",
                    "description": "Only show commits affecting this file/directory"
                },
                "author": {
                    "type": "string",
                    "description": "Filter by author name or email"
                },
                "since": {
                    "type": "string",
                    "description": "Show commits after this date (e.g., '2024-01-01', '2 weeks ago')"
                },
                "format": {
                    "type": "string",
                    "description": "Output format: 'oneline' (default), 'short', 'full'",
                    "enum": ["oneline", "short", "full"],
                    "default": "oneline"
                }
            }
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let count = input
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(50);
        let path_filter = input.get("path").and_then(|v| v.as_str());
        let author = input.get("author").and_then(|v| v.as_str());
        let since = input.get("since").and_then(|v| v.as_str());
        let format = input
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("oneline");

        let mut args = vec!["log".to_string()];

        match format {
            "oneline" => args.push("--oneline".to_string()),
            "short" => args.push("--format=short".to_string()),
            "full" => args.push("--format=fuller".to_string()),
            _ => args.push("--oneline".to_string()),
        }

        args.push(format!("-{}", count));

        if let Some(a) = author {
            args.push(format!("--author={}", a));
        }
        if let Some(s) = since {
            args.push(format!("--since={}", s));
        }
        if let Some(p) = path_filter {
            args.push("--".to_string());
            args.push(p.to_string());
        }

        let output = tokio::process::Command::new("git")
            .args(&args)
            .current_dir(&self.workspace)
            .output()
            .await?;

        let log = String::from_utf8_lossy(&output.stdout);
        if log.trim().is_empty() {
            return Ok("No commits found matching the criteria.".to_string());
        }

        Ok(log.to_string())
    }
}

// ============================================================================
// git_commit
// ============================================================================

pub struct GitCommitTool {
    workspace: PathBuf,
}

impl GitCommitTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Stage files and create a git commit. If no files are specified, commits all currently staged changes."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Commit message"
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Files to stage before committing. If omitted, commits currently staged changes."
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'message' parameter"))?;
        let files: Vec<String> = input
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Stage files if specified
        if !files.is_empty() {
            let mut add_args = vec!["add".to_string()];
            add_args.extend(files.clone());

            let add_output = tokio::process::Command::new("git")
                .args(&add_args)
                .current_dir(&self.workspace)
                .output()
                .await?;

            if !add_output.status.success() {
                let stderr = String::from_utf8_lossy(&add_output.stderr);
                anyhow::bail!("git add failed: {}", stderr);
            }
        }

        // Create commit
        let output = tokio::process::Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.workspace)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            anyhow::bail!("git commit failed: {}", stderr);
        }

        Ok(format!("{}{}", stdout, stderr))
    }
}

// ============================================================================
// git_branch
// ============================================================================

pub struct GitBranchTool {
    workspace: PathBuf,
}

impl GitBranchTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for GitBranchTool {
    fn name(&self) -> &str {
        "git_branch"
    }

    fn description(&self) -> &str {
        "Manage git branches: list, create, or switch branches."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform: 'list', 'create', or 'switch'",
                    "enum": ["list", "create", "switch"]
                },
                "name": {
                    "type": "string",
                    "description": "Branch name (required for 'create' and 'switch')"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' parameter"))?;
        let name = input.get("name").and_then(|v| v.as_str());

        let output = match action {
            "list" => {
                tokio::process::Command::new("git")
                    .args(["branch", "-a", "--sort=-committerdate"])
                    .current_dir(&self.workspace)
                    .output()
                    .await?
            }
            "create" => {
                let branch_name =
                    name.ok_or_else(|| anyhow::anyhow!("'name' is required for 'create'"))?;
                tokio::process::Command::new("git")
                    .args(["checkout", "-b", branch_name])
                    .current_dir(&self.workspace)
                    .output()
                    .await?
            }
            "switch" => {
                let branch_name =
                    name.ok_or_else(|| anyhow::anyhow!("'name' is required for 'switch'"))?;
                tokio::process::Command::new("git")
                    .args(["checkout", branch_name])
                    .current_dir(&self.workspace)
                    .output()
                    .await?
            }
            _ => anyhow::bail!("Unknown action: {}. Use 'list', 'create', or 'switch'.", action),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            anyhow::bail!("git {} failed: {}", action, stderr);
        }

        Ok(format!("{}{}", stdout, stderr))
    }
}

// ============================================================================
// background_exec
// ============================================================================

use std::sync::Arc;
use tokio::sync::RwLock;

/// A background process tracked by the process management tools
pub struct BackgroundProcess {
    pub label: String,
    pub command: String,
    pub started_at: std::time::Instant,
    pub output: Arc<RwLock<BackgroundOutput>>,
    pub handle: Option<tokio::task::JoinHandle<()>>,
    pub child_id: Option<u32>,
}

/// Output buffer for a background process
pub struct BackgroundOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub done: bool,
}

/// Shared registry of background processes
pub type ProcessRegistry = Arc<RwLock<std::collections::HashMap<String, BackgroundProcess>>>;

pub fn new_process_registry() -> ProcessRegistry {
    Arc::new(RwLock::new(std::collections::HashMap::new()))
}

pub struct BackgroundExecTool {
    workspace: PathBuf,
    processes: ProcessRegistry,
}

impl BackgroundExecTool {
    pub fn new(workspace: PathBuf, processes: ProcessRegistry) -> Self {
        Self {
            workspace,
            processes,
        }
    }
}

#[async_trait]
impl Tool for BackgroundExecTool {
    fn name(&self) -> &str {
        "background_exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the background. Returns a process ID that can be used with process_status and process_signal to monitor and control the process."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute in the background"
                },
                "label": {
                    "type": "string",
                    "description": "Optional label to identify this process"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?
            .to_string();
        let label = input
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let process_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

        let output_buf = Arc::new(RwLock::new(BackgroundOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            done: false,
        }));

        let workspace = self.workspace.clone();
        let cmd_clone = command.clone();
        let output_clone = output_buf.clone();

        let handle = tokio::spawn(async move {
            let result = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd_clone)
                .current_dir(&workspace)
                .output()
                .await;

            let mut out = output_clone.write().await;
            match result {
                Ok(output) => {
                    out.stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    out.stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    out.exit_code = output.status.code();
                }
                Err(e) => {
                    out.stderr = format!("Failed to execute: {}", e);
                    out.exit_code = Some(-1);
                }
            }
            out.done = true;
        });

        let process = BackgroundProcess {
            label: label.clone(),
            command: command.clone(),
            started_at: std::time::Instant::now(),
            output: output_buf,
            handle: Some(handle),
            child_id: None,
        };

        self.processes.write().await.insert(process_id.clone(), process);

        Ok(json!({
            "process_id": process_id,
            "command": command,
            "label": label,
            "status": "running"
        })
        .to_string())
    }
}

// ============================================================================
// process_status
// ============================================================================

pub struct ProcessStatusTool {
    processes: ProcessRegistry,
}

impl ProcessStatusTool {
    pub fn new(processes: ProcessRegistry) -> Self {
        Self { processes }
    }
}

#[async_trait]
impl Tool for ProcessStatusTool {
    fn name(&self) -> &str {
        "process_status"
    }

    fn description(&self) -> &str {
        "Check the status of a background process. Returns whether it's running or completed, its exit code, and the last lines of output."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "process_id": {
                    "type": "string",
                    "description": "The process ID returned by background_exec"
                },
                "tail_lines": {
                    "type": "integer",
                    "description": "Number of output lines to return from the end (default: 50)",
                    "default": 50
                }
            },
            "required": ["process_id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let process_id = input
            .get("process_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'process_id' parameter"))?;
        let tail_lines = input
            .get("tail_lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let processes = self.processes.read().await;
        let process = processes
            .get(process_id)
            .ok_or_else(|| anyhow::anyhow!("Process not found: {}", process_id))?;

        let output = process.output.read().await;
        let elapsed = process.started_at.elapsed();

        let status = if output.done { "completed" } else { "running" };

        // Get tail of stdout
        let stdout_lines: Vec<&str> = output.stdout.lines().collect();
        let stdout_tail: String = stdout_lines
            .iter()
            .rev()
            .take(tail_lines)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");

        let stderr_lines: Vec<&str> = output.stderr.lines().collect();
        let stderr_tail: String = stderr_lines
            .iter()
            .rev()
            .take(tail_lines)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");

        Ok(json!({
            "process_id": process_id,
            "label": process.label,
            "command": process.command,
            "status": status,
            "exit_code": output.exit_code,
            "elapsed_seconds": elapsed.as_secs(),
            "stdout": stdout_tail,
            "stderr": stderr_tail
        })
        .to_string())
    }
}

// ============================================================================
// process_signal
// ============================================================================

pub struct ProcessSignalTool {
    processes: ProcessRegistry,
}

impl ProcessSignalTool {
    pub fn new(processes: ProcessRegistry) -> Self {
        Self { processes }
    }
}

#[async_trait]
impl Tool for ProcessSignalTool {
    fn name(&self) -> &str {
        "process_signal"
    }

    fn description(&self) -> &str {
        "Send a signal to a background process. Use 'kill' to terminate or 'stop' to suspend."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "process_id": {
                    "type": "string",
                    "description": "The process ID returned by background_exec"
                },
                "signal": {
                    "type": "string",
                    "description": "Signal to send: 'kill' (default) or 'stop'",
                    "enum": ["kill", "stop"],
                    "default": "kill"
                }
            },
            "required": ["process_id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let process_id = input
            .get("process_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'process_id' parameter"))?;
        let signal = input
            .get("signal")
            .and_then(|v| v.as_str())
            .unwrap_or("kill");

        let mut processes = self.processes.write().await;
        let process = processes
            .get_mut(process_id)
            .ok_or_else(|| anyhow::anyhow!("Process not found: {}", process_id))?;

        // Abort the tokio task
        if let Some(handle) = process.handle.take() {
            handle.abort();
        }

        // Mark as done
        let mut output = process.output.write().await;
        if !output.done {
            output.done = true;
            output.exit_code = Some(-9);
            output.stderr.push_str(&format!("\n[Process {} by user]", signal));
        }

        Ok(json!({
            "process_id": process_id,
            "signal": signal,
            "result": "signaled"
        })
        .to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    #[test]
    fn test_resolve_workspace_path_valid() {
        let (_dir, workspace) = setup();
        let result = resolve_workspace_path(&workspace, "test.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_workspace_path_rejects_escape() {
        let (_dir, workspace) = setup();
        let result = resolve_workspace_path(&workspace, "../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_workspace_path_rejects_absolute() {
        let (_dir, workspace) = setup();
        let result = resolve_workspace_path(&workspace, "/etc/passwd");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_write_and_read() {
        let (_dir, workspace) = setup();

        let write_tool = FileWriteTool::new(workspace.clone());
        let result = write_tool
            .execute(json!({
                "path": "hello.txt",
                "content": "Hello, World!"
            }))
            .await
            .unwrap();
        assert!(result.contains("13 bytes"));

        let read_tool = FileReadTool::new(workspace);
        let result = read_tool
            .execute(json!({"path": "hello.txt"}))
            .await
            .unwrap();
        assert!(result.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_file_edit() {
        let (_dir, workspace) = setup();

        // Create a file first
        tokio::fs::write(workspace.join("test.py"), "print('hello')\nprint('world')\n")
            .await
            .unwrap();

        let tool = FileEditTool::new(workspace.clone());
        let result = tool
            .execute(json!({
                "path": "test.py",
                "old_text": "print('hello')",
                "new_text": "print('goodbye')"
            }))
            .await
            .unwrap();
        assert!(result.contains("Replaced 1"));

        let content = tokio::fs::read_to_string(workspace.join("test.py"))
            .await
            .unwrap();
        assert!(content.contains("print('goodbye')"));
        assert!(content.contains("print('world')"));
    }

    #[tokio::test]
    async fn test_file_delete() {
        let (_dir, workspace) = setup();

        tokio::fs::write(workspace.join("deleteme.txt"), "temp")
            .await
            .unwrap();

        let tool = FileDeleteTool::new(workspace.clone());
        let result = tool
            .execute(json!({"path": "deleteme.txt"}))
            .await
            .unwrap();
        assert!(result.contains("Deleted"));
        assert!(!workspace.join("deleteme.txt").exists());
    }

    #[tokio::test]
    async fn test_list_directory() {
        let (_dir, workspace) = setup();

        tokio::fs::write(workspace.join("a.txt"), "").await.unwrap();
        tokio::fs::write(workspace.join("b.txt"), "").await.unwrap();
        tokio::fs::create_dir(workspace.join("subdir")).await.unwrap();

        let tool = ListDirectoryTool::new(workspace);
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.contains("subdir/"));
        assert!(result.contains("a.txt"));
        assert!(result.contains("b.txt"));
    }

    #[tokio::test]
    async fn test_file_search() {
        let (_dir, workspace) = setup();

        tokio::fs::write(workspace.join("main.rs"), "fn main() {\n    println!(\"hello\");\n}\n")
            .await
            .unwrap();

        let tool = FileSearchTool::new(workspace);
        let result = tool
            .execute(json!({
                "pattern": "println",
                "glob": "*.rs"
            }))
            .await
            .unwrap();
        assert!(result.contains("main.rs:2"));
        assert!(result.contains("println"));
    }
}
