//! Skill registry - Manages loaded skills and their activation state

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::config::SkillsConfig;
use crate::skills::loader::{expand_path, SkillLoader};
use crate::skills::{SkillInfo, SkillRef};

/// Registry for managing loaded skills
pub struct SkillRegistry {
    /// All discovered skills (keyed by name)
    skills: HashMap<String, SkillRef>,

    /// Currently active skills (enabled and loaded)
    active_skills: HashSet<String>,

    /// Skill directories that were scanned
    directories: Vec<PathBuf>,
}

impl SkillRegistry {
    /// Create a new empty skill registry
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            active_skills: HashSet::new(),
            directories: Vec::new(),
        }
    }

    /// Create a skill registry from configuration
    pub fn from_config(config: &SkillsConfig) -> anyhow::Result<Self> {
        let mut registry = Self::new();

        // Expand and add directories
        for dir in &config.directories {
            let expanded = expand_path(dir);
            registry.directories.push(PathBuf::from(&expanded));
        }

        // Scan all directories for skills
        registry.scan_all()?;

        // Apply enabled/disabled lists
        if config.enabled_skills.is_empty() {
            // If no explicit enable list, enable all discovered skills
            // (unless they're in the disabled list)
            for name in registry.skills.keys().cloned().collect::<Vec<_>>() {
                if !config.disabled_skills.contains(&name) {
                    if let Err(e) = registry.enable(&name) {
                        tracing::warn!("Failed to enable skill '{}': {}", name, e);
                    }
                }
            }
        } else {
            // Enable only explicitly listed skills
            for name in &config.enabled_skills {
                if config.disabled_skills.contains(name) {
                    tracing::warn!(
                        "Skill '{}' is in both enabled and disabled lists, skipping",
                        name
                    );
                    continue;
                }
                if let Err(e) = registry.enable(name) {
                    tracing::warn!("Failed to enable skill '{}': {}", name, e);
                }
            }
        }

        Ok(registry)
    }

    /// Scan all configured directories for skills
    pub fn scan_all(&mut self) -> anyhow::Result<()> {
        for dir in &self.directories.clone() {
            match SkillLoader::scan_directory(dir) {
                Ok(skills) => {
                    for skill in skills {
                        let name = skill.metadata.name.clone();
                        if self.skills.contains_key(&name) {
                            tracing::warn!(
                                "Duplicate skill '{}' found at {}, using first occurrence",
                                name,
                                skill.source_path.display()
                            );
                        } else {
                            self.skills.insert(name, skill);
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("Skipping directory {}: {}", dir.display(), e);
                }
            }
        }
        Ok(())
    }

    /// Add a directory to scan for skills
    pub fn add_directory(&mut self, path: PathBuf) {
        if !self.directories.contains(&path) {
            self.directories.push(path);
        }
    }

    /// Enable a skill (load full content)
    pub fn enable(&mut self, name: &str) -> anyhow::Result<()> {
        let skill = self.skills.get_mut(name).ok_or_else(|| {
            anyhow::anyhow!("Skill '{}' not found", name)
        })?;

        // Activate the skill (load full instructions)
        SkillLoader::activate_skill(skill)?;

        self.active_skills.insert(name.to_string());
        tracing::info!("Enabled skill: {}", name);

        Ok(())
    }

    /// Disable a skill
    pub fn disable(&mut self, name: &str) {
        if self.active_skills.remove(name) {
            tracing::info!("Disabled skill: {}", name);
        }
    }

    /// Check if a skill is active
    pub fn is_active(&self, name: &str) -> bool {
        self.active_skills.contains(name)
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<&SkillRef> {
        self.skills.get(name)
    }

    /// Get a mutable skill by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut SkillRef> {
        self.skills.get_mut(name)
    }

    /// List all discovered skills
    pub fn list(&self) -> Vec<&SkillRef> {
        self.skills.values().collect()
    }

    /// List only active skills
    pub fn list_active(&self) -> Vec<&SkillRef> {
        self.active_skills
            .iter()
            .filter_map(|name| self.skills.get(name))
            .collect()
    }

    /// Get detailed info for all skills
    pub fn list_info(&self) -> Vec<SkillInfo> {
        self.skills
            .values()
            .map(|skill| {
                let mut info = SkillInfo::from(skill);
                info.is_active = self.is_active(&skill.metadata.name);
                info
            })
            .collect()
    }

    /// Get the number of discovered skills
    pub fn count(&self) -> usize {
        self.skills.len()
    }

    /// Get the number of active skills
    pub fn active_count(&self) -> usize {
        self.active_skills.len()
    }

    /// Get instructions for all active skills (for injection into agent prompt)
    ///
    /// Returns a formatted string containing all active skill instructions
    pub fn get_active_instructions(&self) -> String {
        let mut instructions = String::new();

        for name in &self.active_skills {
            if let Some(skill) = self.skills.get(name) {
                if let Some(content) = skill.instructions() {
                    instructions.push_str(&format!(
                        "\n## Skill: {}\n\n{}\n",
                        skill.metadata.name, content
                    ));
                }
            }
        }

        instructions
    }

    /// Get a summary of all discovered skills (names and descriptions)
    ///
    /// This is a lightweight summary for initial context loading
    pub fn get_skill_summary(&self) -> String {
        let mut summary = String::new();

        for skill in self.skills.values() {
            let status = if self.is_active(&skill.metadata.name) {
                "[active]"
            } else {
                "[inactive]"
            };
            summary.push_str(&format!(
                "- {} {}: {}\n",
                skill.metadata.name, status, skill.metadata.description
            ));
        }

        summary
    }

    /// Load a reference file from a skill
    pub fn load_reference(&self, skill_name: &str, ref_name: &str) -> anyhow::Result<String> {
        let skill = self.skills.get(skill_name).ok_or_else(|| {
            anyhow::anyhow!("Skill '{}' not found", skill_name)
        })?;

        SkillLoader::load_reference(skill, ref_name)
    }

    /// Load a script from a skill
    pub fn load_script(&self, skill_name: &str, script_name: &str) -> anyhow::Result<String> {
        let skill = self.skills.get(skill_name).ok_or_else(|| {
            anyhow::anyhow!("Skill '{}' not found", skill_name)
        })?;

        SkillLoader::load_script(skill, script_name)
    }

    /// Get the scanned directories
    pub fn directories(&self) -> &[PathBuf] {
        &self.directories
    }

    /// Rescan all directories for new skills
    pub fn rescan(&mut self) -> anyhow::Result<usize> {
        let old_count = self.skills.len();

        // Clear and rescan
        self.skills.clear();
        self.scan_all()?;

        // Re-enable previously active skills that still exist
        let active = self.active_skills.clone();
        self.active_skills.clear();

        for name in active {
            if self.skills.contains_key(&name) {
                let _ = self.enable(&name);
            } else {
                tracing::warn!("Previously active skill '{}' no longer exists", name);
            }
        }

        let new_count = self.skills.len();
        Ok(new_count.saturating_sub(old_count))
    }

    /// Register a skill directly (e.g., from sync)
    pub fn register(&mut self, skill: SkillRef) {
        let name = skill.metadata.name.clone();
        self.skills.insert(name, skill);
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
