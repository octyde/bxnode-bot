//! Skill sync - Fetch and update skills from GitHub repositories

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::skills::loader::SkillLoader;
use crate::skills::SkillRef;

/// Skill sync source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSyncSource {
    /// GitHub repository in owner/repo format
    pub repo: String,

    /// Subdirectory within repo to sync from (optional)
    #[serde(default)]
    pub path: Option<String>,

    /// Specific skills to sync (empty = all)
    #[serde(default)]
    pub skills: Vec<String>,

    /// Branch to sync from (default: main)
    #[serde(default = "default_branch")]
    pub branch: String,
}

fn default_branch() -> String {
    "main".to_string()
}

/// Report from a sync operation
#[derive(Debug, Default)]
pub struct SyncReport {
    /// Skills that were synced successfully
    pub synced: Vec<String>,

    /// Skills that were skipped (already exist)
    pub skipped: Vec<String>,

    /// Errors encountered during sync
    pub errors: Vec<(String, String)>,
}

impl SyncReport {
    /// Check if the sync was completely successful
    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get total number of skills processed
    pub fn total(&self) -> usize {
        self.synced.len() + self.skipped.len() + self.errors.len()
    }
}

/// Skill syncer - fetches skills from GitHub repositories
pub struct SkillSyncer {
    client: reqwest::Client,
    target_dir: PathBuf,
}

impl SkillSyncer {
    /// Create a new skill syncer
    pub fn new(target_dir: PathBuf) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("bxnode-bot")
                .build()
                .expect("Failed to create HTTP client"),
            target_dir,
        }
    }

    /// Sync skills from the awesome-openclaw-skills list
    ///
    /// Fetches the README and parses skill links from it.
    pub async fn sync_from_awesome_list(&self, force: bool) -> anyhow::Result<SyncReport> {
        const AWESOME_URL: &str =
            "https://raw.githubusercontent.com/VoltAgent/awesome-openclaw-skills/main/README.md";

        tracing::info!("Fetching awesome-openclaw-skills list...");

        let content = self
            .client
            .get(AWESOME_URL)
            .send()
            .await?
            .text()
            .await?;

        let skill_repos = Self::parse_awesome_list(&content)?;
        tracing::info!("Found {} skill references in awesome list", skill_repos.len());

        let mut report = SyncReport::default();

        for (name, repo_path) in skill_repos {
            match self.sync_skill_from_repo(&name, &repo_path, force).await {
                Ok(synced) => {
                    if synced {
                        report.synced.push(name);
                    } else {
                        report.skipped.push(name);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to sync skill '{}': {}", name, e);
                    report.errors.push((name, e.to_string()));
                }
            }
        }

        Ok(report)
    }

    /// Sync skills from a configured source
    pub async fn sync_from_source(
        &self,
        source: &SkillSyncSource,
        force: bool,
    ) -> anyhow::Result<SyncReport> {
        tracing::info!("Syncing from {}", source.repo);

        let mut report = SyncReport::default();

        if source.skills.is_empty() {
            // Sync all skills from the repository
            let skills = self.list_skills_in_repo(source).await?;

            for skill_name in skills {
                let skill_path = if let Some(ref path) = source.path {
                    format!("{}/{}", path, skill_name)
                } else {
                    skill_name.clone()
                };

                match self
                    .sync_skill_from_github(&source.repo, &skill_path, &source.branch, force)
                    .await
                {
                    Ok(synced) => {
                        if synced {
                            report.synced.push(skill_name);
                        } else {
                            report.skipped.push(skill_name);
                        }
                    }
                    Err(e) => {
                        report.errors.push((skill_name, e.to_string()));
                    }
                }
            }
        } else {
            // Sync specific skills
            for skill_name in &source.skills {
                let skill_path = if let Some(ref path) = source.path {
                    format!("{}/{}", path, skill_name)
                } else {
                    skill_name.clone()
                };

                match self
                    .sync_skill_from_github(&source.repo, &skill_path, &source.branch, force)
                    .await
                {
                    Ok(synced) => {
                        if synced {
                            report.synced.push(skill_name.clone());
                        } else {
                            report.skipped.push(skill_name.clone());
                        }
                    }
                    Err(e) => {
                        report.errors.push((skill_name.clone(), e.to_string()));
                    }
                }
            }
        }

        Ok(report)
    }

    /// Sync a single skill from a GitHub repository
    ///
    /// Returns true if the skill was synced, false if it was skipped.
    pub async fn sync_skill_from_github(
        &self,
        repo: &str,
        skill_path: &str,
        branch: &str,
        force: bool,
    ) -> anyhow::Result<bool> {
        let skill_name = skill_path
            .split('/')
            .last()
            .ok_or_else(|| anyhow::anyhow!("Invalid skill path"))?;

        let target_dir = self.target_dir.join(skill_name);

        // Check if skill already exists
        if target_dir.exists() && !force {
            tracing::debug!("Skill '{}' already exists, skipping", skill_name);
            return Ok(false);
        }

        // Fetch SKILL.md content
        let skill_md_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/SKILL.md",
            repo, branch, skill_path
        );

        let response = self.client.get(&skill_md_url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to fetch SKILL.md from {}: {}",
                skill_md_url,
                response.status()
            );
        }

        let skill_md_content = response.text().await?;

        // Create skill directory
        std::fs::create_dir_all(&target_dir)?;

        // Write SKILL.md
        let skill_md_path = target_dir.join("SKILL.md");
        let mut file = std::fs::File::create(&skill_md_path)?;
        file.write_all(skill_md_content.as_bytes())?;

        // Try to fetch optional directories (references, scripts, assets)
        for subdir in &["references", "scripts", "assets"] {
            if let Err(e) = self
                .sync_directory(repo, skill_path, subdir, branch, &target_dir)
                .await
            {
                tracing::debug!(
                    "Could not sync {} for skill '{}': {}",
                    subdir,
                    skill_name,
                    e
                );
            }
        }

        tracing::info!("Synced skill: {}", skill_name);
        Ok(true)
    }

    /// Sync a skill from the openclaw/skills repository
    async fn sync_skill_from_repo(
        &self,
        _name: &str,
        repo_path: &str,
        force: bool,
    ) -> anyhow::Result<bool> {
        // Parse the repo path (e.g., "openclaw/skills/tree/main/skills/author/skill-name")
        // to extract repo, branch, and path
        let parts: Vec<&str> = repo_path.split('/').collect();

        if parts.len() < 5 {
            anyhow::bail!("Invalid repository path format: {}", repo_path);
        }

        let repo = format!("{}/{}", parts[0], parts[1]);

        // Find "tree" or "blob" in path
        let tree_idx = parts.iter().position(|&p| p == "tree" || p == "blob");

        let (branch, skill_path) = if let Some(idx) = tree_idx {
            let branch = parts.get(idx + 1).unwrap_or(&"main");
            let path_parts: Vec<&str> = parts[idx + 2..].to_vec();
            (branch.to_string(), path_parts.join("/"))
        } else {
            ("main".to_string(), parts[2..].join("/"))
        };

        self.sync_skill_from_github(&repo, &skill_path, &branch, force)
            .await
    }

    /// Sync a subdirectory from a skill
    async fn sync_directory(
        &self,
        repo: &str,
        skill_path: &str,
        subdir: &str,
        branch: &str,
        target_dir: &Path,
    ) -> anyhow::Result<()> {
        // Use GitHub API to list directory contents
        let api_url = format!(
            "https://api.github.com/repos/{}/contents/{}/{}?ref={}",
            repo, skill_path, subdir, branch
        );

        let response = self
            .client
            .get(&api_url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(()); // Directory might not exist, which is fine
        }

        let files: Vec<GitHubFile> = response.json().await?;

        let subdir_path = target_dir.join(subdir);
        std::fs::create_dir_all(&subdir_path)?;

        for file in files {
            if file.file_type == "file" {
                if let Some(download_url) = file.download_url {
                    let content = self.client.get(&download_url).send().await?.text().await?;

                    let file_path = subdir_path.join(&file.name);
                    let mut f = std::fs::File::create(&file_path)?;
                    f.write_all(content.as_bytes())?;
                }
            }
        }

        Ok(())
    }

    /// List skills in a repository
    async fn list_skills_in_repo(&self, source: &SkillSyncSource) -> anyhow::Result<Vec<String>> {
        let path = source.path.as_deref().unwrap_or("");
        let api_url = format!(
            "https://api.github.com/repos/{}/contents/{}?ref={}",
            source.repo, path, source.branch
        );

        let response = self
            .client
            .get(&api_url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to list skills in {}: {}",
                source.repo,
                response.status()
            );
        }

        let entries: Vec<GitHubFile> = response.json().await?;

        let skills: Vec<String> = entries
            .into_iter()
            .filter(|e| e.file_type == "dir")
            .map(|e| e.name)
            .collect();

        Ok(skills)
    }

    /// Parse skill references from the awesome-openclaw-skills README
    fn parse_awesome_list(content: &str) -> anyhow::Result<Vec<(String, String)>> {
        let mut skills = Vec::new();

        // Match markdown links to skill pages
        // Format: [skill-name](https://github.com/openclaw/skills/tree/main/skills/author/skill-name/SKILL.md)
        let link_pattern =
            regex::Regex::new(r"\[([^\]]+)\]\((https://github\.com/([^)]+))\)")?;

        for cap in link_pattern.captures_iter(content) {
            let name = cap.get(1).map(|m| m.as_str().to_string());
            let url = cap.get(3).map(|m| m.as_str().to_string());

            if let (Some(name), Some(path)) = (name, url) {
                // Filter for skill-like paths (contain "skills" and "SKILL.md" or end with skill name)
                if path.contains("/skills/") || path.ends_with("/SKILL.md") {
                    // Clean up the path to get the skill directory
                    let clean_path = path
                        .replace("/SKILL.md", "")
                        .replace("/blob/", "/tree/");
                    skills.push((name, clean_path));
                }
            }
        }

        Ok(skills)
    }

    /// Install a skill from a GitHub URL
    pub async fn install_from_url(&self, url: &str, force: bool) -> anyhow::Result<SkillRef> {
        // Parse GitHub URL
        // Formats:
        // - https://github.com/owner/repo/tree/branch/path/to/skill
        // - https://github.com/owner/repo/blob/branch/path/to/skill/SKILL.md
        // - owner/repo/path/to/skill

        let (repo, path, branch) = Self::parse_github_url(url)?;

        let synced = self
            .sync_skill_from_github(&repo, &path, &branch, force)
            .await?;

        if !synced {
            anyhow::bail!("Skill already exists. Use --force to overwrite.");
        }

        // Parse the installed skill
        let skill_name = path.split('/').last().unwrap_or(&path);
        let skill_path = self.target_dir.join(skill_name).join("SKILL.md");

        SkillLoader::parse_skill_metadata(&skill_path)
    }

    /// Parse a GitHub URL into (repo, path, branch)
    fn parse_github_url(url: &str) -> anyhow::Result<(String, String, String)> {
        let url = url
            .trim()
            .trim_start_matches("https://github.com/")
            .trim_start_matches("github.com/");

        let parts: Vec<&str> = url.split('/').collect();

        if parts.len() < 2 {
            anyhow::bail!("Invalid GitHub URL format");
        }

        let repo = format!("{}/{}", parts[0], parts[1]);

        // Check for tree/blob in URL
        if parts.len() > 3 && (parts[2] == "tree" || parts[2] == "blob") {
            let branch = parts[3].to_string();
            let path = parts[4..].join("/").replace("/SKILL.md", "");
            Ok((repo, path, branch))
        } else if parts.len() > 2 {
            // Assume main branch
            let path = parts[2..].join("/").replace("/SKILL.md", "");
            Ok((repo, path, "main".to_string()))
        } else {
            anyhow::bail!("Invalid GitHub URL: missing skill path");
        }
    }

    /// Get the target directory
    pub fn target_dir(&self) -> &Path {
        &self.target_dir
    }
}

/// GitHub API file response
#[derive(Debug, Deserialize)]
struct GitHubFile {
    name: String,
    #[serde(rename = "type")]
    file_type: String,
    download_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url() {
        // Full URL with tree
        let (repo, path, branch) =
            SkillSyncer::parse_github_url("https://github.com/openclaw/skills/tree/main/skills/author/my-skill")
                .unwrap();
        assert_eq!(repo, "openclaw/skills");
        assert_eq!(path, "skills/author/my-skill");
        assert_eq!(branch, "main");

        // URL with blob
        let (repo, path, branch) =
            SkillSyncer::parse_github_url("https://github.com/user/repo/blob/dev/path/skill/SKILL.md")
                .unwrap();
        assert_eq!(repo, "user/repo");
        assert_eq!(path, "path/skill");
        assert_eq!(branch, "dev");

        // Short format
        let (repo, path, branch) =
            SkillSyncer::parse_github_url("owner/repo/skills/my-skill").unwrap();
        assert_eq!(repo, "owner/repo");
        assert_eq!(path, "skills/my-skill");
        assert_eq!(branch, "main");
    }
}
