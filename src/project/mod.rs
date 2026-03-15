//! Project module - Coding project workspace management
//!
//! Projects represent coding workspaces with associated configuration.
//! Storage layout: `base_dir/{name}/project.json`
//! Workspace layout: `workspace_base_dir/{name}/` (actual code files)

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Rich project definition with workspace and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Project name (alphanumeric + hyphens/underscores)
    pub name: String,

    /// Workspace directory on disk (absolute path where code lives)
    pub workspace_dir: PathBuf,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Optional per-project model override (e.g., "anthropic/claude-sonnet-4")
    #[serde(default)]
    pub model: Option<String>,

    /// Optional per-project system prompt override
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Whether coding tools are enabled for this project
    #[serde(default = "default_true")]
    pub coding_tools_enabled: bool,

    /// Whether shell command execution is allowed
    #[serde(default)]
    pub shell_enabled: bool,

    /// Additional metadata
    #[serde(default)]
    pub metadata: serde_json::Value,

    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

/// Validates that a project name contains only allowed characters
pub fn validate_project_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        && name != "."
        && name != ".."
}

/// Project store — handles persistent project storage
pub struct ProjectStore {
    /// Base directory for project metadata (e.g., ~/.bxnode-bot/projects/)
    base_dir: PathBuf,

    /// Base directory for project workspaces (e.g., ~/projects/)
    workspace_base_dir: PathBuf,
}

impl ProjectStore {
    pub fn new(base_dir: PathBuf, workspace_base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            workspace_base_dir,
        }
    }

    /// Ensure base directories exist
    pub async fn ensure_base_dirs(&self) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        tokio::fs::create_dir_all(&self.workspace_base_dir).await?;
        Ok(())
    }

    /// Get the metadata directory for a project
    fn project_meta_dir(&self, name: &str) -> PathBuf {
        self.base_dir.join(name)
    }

    /// Get the metadata file path for a project
    fn project_meta_path(&self, name: &str) -> PathBuf {
        self.project_meta_dir(name).join("project.json")
    }

    /// Resolve the default workspace directory for a project name
    pub fn resolve_workspace_dir(&self, name: &str) -> PathBuf {
        self.workspace_base_dir.join(name)
    }

    /// Get the workspace base directory
    pub fn workspace_base_dir(&self) -> &PathBuf {
        &self.workspace_base_dir
    }

    /// Create a new project (saves metadata and creates workspace directory)
    pub async fn create(&self, project: &Project) -> anyhow::Result<()> {
        if !validate_project_name(&project.name) {
            anyhow::bail!("Invalid project name: {}", project.name);
        }

        let meta_dir = self.project_meta_dir(&project.name);
        if meta_dir.exists() {
            anyhow::bail!("Project '{}' already exists", project.name);
        }

        // Create metadata directory and save
        tokio::fs::create_dir_all(&meta_dir).await?;
        let content = serde_json::to_string_pretty(project)?;
        tokio::fs::write(self.project_meta_path(&project.name), content).await?;

        // Create workspace directory
        tokio::fs::create_dir_all(&project.workspace_dir).await?;

        Ok(())
    }

    /// Load project metadata by name
    pub async fn load(&self, name: &str) -> anyhow::Result<Option<Project>> {
        let path = self.project_meta_path(name);
        if !path.exists() {
            return Ok(None);
        }
        let content = tokio::fs::read_to_string(&path).await?;
        let project: Project = serde_json::from_str(&content)?;
        Ok(Some(project))
    }

    /// Update project metadata
    pub async fn update(&self, project: &Project) -> anyhow::Result<()> {
        let path = self.project_meta_path(&project.name);
        if !path.exists() {
            anyhow::bail!("Project '{}' not found", project.name);
        }
        let content = serde_json::to_string_pretty(project)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }

    /// Delete a project's metadata (does NOT delete the workspace directory)
    pub async fn delete(&self, name: &str) -> anyhow::Result<()> {
        if name == "default" {
            anyhow::bail!("Cannot delete the default project");
        }
        let meta_dir = self.project_meta_dir(name);
        if meta_dir.exists() {
            tokio::fs::remove_dir_all(&meta_dir).await?;
        }
        Ok(())
    }

    /// List all projects
    pub async fn list(&self) -> anyhow::Result<Vec<Project>> {
        let mut projects = vec![];

        if !self.base_dir.exists() {
            return Ok(projects);
        }

        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Ok(Some(project)) = self.load(&name).await {
                    projects.push(project);
                }
            }
        }

        // Sort by name, default first
        projects.sort_by(|a, b| {
            if a.name == "default" {
                std::cmp::Ordering::Less
            } else if b.name == "default" {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });

        Ok(projects)
    }

    /// List project names only
    pub async fn list_names(&self) -> anyhow::Result<Vec<String>> {
        let projects = self.list().await?;
        Ok(projects.into_iter().map(|p| p.name).collect())
    }

    /// Check if a project exists
    pub async fn exists(&self, name: &str) -> bool {
        self.project_meta_path(name).exists()
    }
}
