//! Skill loader - Parse SKILL.md files and scan directories

use std::path::Path;

use crate::skills::{SkillMetadata, SkillRef};

/// Skill loader for parsing SKILL.md files and scanning directories
pub struct SkillLoader;

impl SkillLoader {
    /// Scan a directory for skill folders containing SKILL.md files
    ///
    /// Returns a list of SkillRef with metadata only (Level 1 loading)
    pub fn scan_directory(path: &Path) -> anyhow::Result<Vec<SkillRef>> {
        let mut skills = Vec::new();

        if !path.exists() {
            tracing::debug!("Skill directory does not exist: {}", path.display());
            return Ok(skills);
        }

        if !path.is_dir() {
            anyhow::bail!("Path is not a directory: {}", path.display());
        }

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let skill_dir = entry.path();

            if skill_dir.is_dir() {
                let skill_md = skill_dir.join("SKILL.md");
                if skill_md.exists() {
                    match Self::parse_skill_metadata(&skill_md) {
                        Ok(skill_ref) => {
                            tracing::debug!(
                                "Discovered skill: {} at {}",
                                skill_ref.metadata.name,
                                skill_dir.display()
                            );
                            skills.push(skill_ref);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to parse SKILL.md at {}: {}",
                                skill_md.display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        Ok(skills)
    }

    /// Parse SKILL.md file and extract metadata only (Level 1)
    ///
    /// This is the initial load - only metadata is extracted to minimize context usage.
    pub fn parse_skill_metadata(path: &Path) -> anyhow::Result<SkillRef> {
        let content = std::fs::read_to_string(path)?;
        let (metadata, _body) = Self::parse_frontmatter(&content)?;

        // Validate metadata
        metadata.validate()?;

        // Verify the directory name matches the skill name
        let skill_dir = path.parent().ok_or_else(|| {
            anyhow::anyhow!("SKILL.md must be in a skill directory")
        })?;

        let dir_name = skill_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid skill directory name"))?;

        if dir_name != metadata.name {
            tracing::warn!(
                "Skill directory name '{}' does not match skill name '{}'",
                dir_name,
                metadata.name
            );
        }

        Ok(SkillRef::new(metadata, skill_dir.to_path_buf()))
    }

    /// Activate a skill by loading its full instructions (Level 2)
    ///
    /// This loads the full markdown body from SKILL.md
    pub fn activate_skill(skill_ref: &mut SkillRef) -> anyhow::Result<()> {
        if skill_ref.is_activated {
            return Ok(());
        }

        let skill_md = skill_ref.source_path.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_md)?;

        let (_, body) = Self::parse_frontmatter(&content)?;
        skill_ref.set_instructions(body);

        tracing::debug!("Activated skill: {}", skill_ref.metadata.name);
        Ok(())
    }

    /// Load a reference file from a skill (Level 3)
    ///
    /// Reference files are loaded on-demand when the agent needs them.
    pub fn load_reference(skill_ref: &SkillRef, ref_name: &str) -> anyhow::Result<String> {
        let ref_path = skill_ref.source_path.join("references").join(ref_name);

        if !ref_path.exists() {
            anyhow::bail!(
                "Reference file '{}' not found in skill '{}'",
                ref_name,
                skill_ref.metadata.name
            );
        }

        let content = std::fs::read_to_string(&ref_path)?;
        tracing::debug!(
            "Loaded reference '{}' from skill '{}'",
            ref_name,
            skill_ref.metadata.name
        );

        Ok(content)
    }

    /// Load a script from a skill
    pub fn load_script(skill_ref: &SkillRef, script_name: &str) -> anyhow::Result<String> {
        let script_path = skill_ref.source_path.join("scripts").join(script_name);

        if !script_path.exists() {
            anyhow::bail!(
                "Script '{}' not found in skill '{}'",
                script_name,
                skill_ref.metadata.name
            );
        }

        let content = std::fs::read_to_string(&script_path)?;
        tracing::debug!(
            "Loaded script '{}' from skill '{}'",
            script_name,
            skill_ref.metadata.name
        );

        Ok(content)
    }

    /// Parse YAML frontmatter from markdown content
    ///
    /// Returns (metadata, body) where body is the markdown content after the frontmatter.
    fn parse_frontmatter(content: &str) -> anyhow::Result<(SkillMetadata, String)> {
        let content = content.trim();

        // Check for frontmatter delimiters
        if !content.starts_with("---") {
            anyhow::bail!("SKILL.md must start with YAML frontmatter (---)");
        }

        // Find the end of frontmatter
        let rest = &content[3..]; // Skip opening "---"
        let end_idx = rest.find("\n---").ok_or_else(|| {
            anyhow::anyhow!("SKILL.md frontmatter must be closed with ---")
        })?;

        let yaml_content = rest[..end_idx].trim();
        let body = rest[end_idx + 4..].trim().to_string(); // Skip "\n---"

        // Parse YAML frontmatter
        let metadata: SkillMetadata = serde_yaml::from_str(yaml_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse SKILL.md frontmatter: {}", e))?;

        Ok((metadata, body))
    }

    /// List all reference files in a skill
    pub fn list_references(skill_ref: &SkillRef) -> Vec<String> {
        let ref_dir = skill_ref.source_path.join("references");
        if !ref_dir.exists() {
            return Vec::new();
        }

        std::fs::read_dir(&ref_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().to_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List all script files in a skill
    pub fn list_scripts(skill_ref: &SkillRef) -> Vec<String> {
        let script_dir = skill_ref.source_path.join("scripts");
        if !script_dir.exists() {
            return Vec::new();
        }

        std::fs::read_dir(&script_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().to_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List all asset files in a skill
    pub fn list_assets(skill_ref: &SkillRef) -> Vec<String> {
        let asset_dir = skill_ref.source_path.join("assets");
        if !asset_dir.exists() {
            return Vec::new();
        }

        std::fs::read_dir(&asset_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().to_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Expand tilde (~) in path to home directory
pub fn expand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }
    path.to_string()
}
