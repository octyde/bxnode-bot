//! Skill output - write SKILL.md files and reference materials
//!
//! This module handles writing extracted skills to the filesystem.

use std::fs;
use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use super::traits::{DefaultOutputFormat, OutputFormat};
use super::types::{ExtractedSkill, GenerationReport, ValidationReport};

/// Skill file writer
///
/// Writes extracted skills to SKILL.md files in the output directory.
pub struct SkillWriter {
    output_dir: PathBuf,
    dry_run: bool,
}

impl SkillWriter {
    /// Create a new writer
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            dry_run: false,
        }
    }

    /// Enable dry run mode (no files written)
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Write all skills using the default output format
    pub fn write_all(&self, skills: &[ExtractedSkill]) -> anyhow::Result<GenerationReport> {
        let format = DefaultOutputFormat::default();
        self.write_all_with_format(skills, &format)
    }

    /// Write all skills using a custom output format
    pub fn write_all_with_format(
        &self,
        skills: &[ExtractedSkill],
        format: &dyn OutputFormat,
    ) -> anyhow::Result<GenerationReport> {
        let start = std::time::Instant::now();
        let mut report = GenerationReport::new();

        if self.dry_run {
            info!("Dry run mode - no files will be written");
        }

        // Ensure output directory exists
        if !self.dry_run {
            fs::create_dir_all(&self.output_dir)?;
        }

        for skill in skills {
            match self.write_skill(skill, format) {
                Ok(()) => {
                    report.skill_names.push(skill.name.clone());
                    report.skills_created += 1;
                    info!("Generated skill: {}", skill.name);
                }
                Err(e) => {
                    warn!("Failed to generate skill '{}': {}", skill.name, e);
                    report.errors.push(format!("{}: {}", skill.name, e));
                }
            }
        }

        report.duration_ms = start.elapsed().as_millis() as u64;
        Ok(report)
    }

    /// Write a single skill
    fn write_skill(&self, skill: &ExtractedSkill, format: &dyn OutputFormat) -> anyhow::Result<()> {
        let skill_dir = self.output_dir.join(&skill.name);

        if self.dry_run {
            info!("[DRY RUN] Would create: {}/SKILL.md", skill_dir.display());
            return Ok(());
        }

        // Create skill directory
        fs::create_dir_all(&skill_dir)?;

        // Generate SKILL.md content using the format
        let skill_md = format.format_skill(skill);
        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, &skill_md)?;
        debug!("Wrote: {}", skill_path.display());

        // Generate reference files
        if !skill.references.is_empty() {
            let ref_dir = skill_dir.join("references");
            fs::create_dir_all(&ref_dir)?;

            for ref_file in &skill.references {
                let ref_path = ref_dir.join(&ref_file.name);
                fs::write(&ref_path, &ref_file.content)?;
                debug!("Wrote reference: {}", ref_path.display());
            }
        }

        Ok(())
    }

    /// Validate generated skills directory
    pub fn validate_skills_dir(skills_dir: &Path) -> anyhow::Result<ValidationReport> {
        let mut report = ValidationReport::default();

        if !skills_dir.exists() {
            anyhow::bail!("Skills directory does not exist: {:?}", skills_dir);
        }

        for entry in fs::read_dir(skills_dir)? {
            let entry = entry?;
            let skill_dir = entry.path();

            if !skill_dir.is_dir() {
                continue;
            }

            let skill_name = skill_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let skill_md_path = skill_dir.join("SKILL.md");
            if !skill_md_path.exists() {
                report
                    .errors
                    .push(format!("{}: Missing SKILL.md file", skill_name));
                report.invalid_count += 1;
                continue;
            }

            // Try to validate the skill file
            match Self::validate_skill_file(&skill_md_path) {
                Ok(()) => {
                    report.valid_count += 1;
                    report.skill_names.push(skill_name.to_string());
                }
                Err(e) => {
                    report.errors.push(format!("{}: {}", skill_name, e));
                    report.invalid_count += 1;
                }
            }
        }

        Ok(report)
    }

    /// Validate a single SKILL.md file
    fn validate_skill_file(path: &Path) -> anyhow::Result<()> {
        let content = fs::read_to_string(path)?;

        // Check for frontmatter
        if !content.starts_with("---") {
            anyhow::bail!("Missing YAML frontmatter");
        }

        // Find frontmatter end
        let rest = &content[3..];
        let end_idx = rest
            .find("\n---")
            .ok_or_else(|| anyhow::anyhow!("Unclosed frontmatter"))?;

        let frontmatter = &rest[..end_idx];

        // Check required fields
        if !frontmatter.contains("name:") {
            anyhow::bail!("Missing 'name' in frontmatter");
        }
        if !frontmatter.contains("description:") {
            anyhow::bail!("Missing 'description' in frontmatter");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skillgen::types::{CodeExample, ImplementationGuide, ImplementationStep};
    use tempfile::tempdir;

    fn create_test_skill() -> ExtractedSkill {
        ExtractedSkill {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            platform: "test".to_string(),
            use_cases: vec!["Testing".to_string()],
            implementation: ImplementationGuide {
                overview: "This is a test skill.".to_string(),
                steps: vec![ImplementationStep {
                    title: "Step 1".to_string(),
                    description: "Do the first thing.".to_string(),
                    code: Some(CodeExample {
                        content: "println!(\"Hello\");".to_string(),
                        language: "rust".to_string(),
                        explanation: "Prints hello.".to_string(),
                    }),
                }],
            },
            related_skills: vec!["other-skill".to_string()],
            references: vec![],
            confidence: 0.9,
            extensions: serde_json::json!({}),
        }
    }

    #[test]
    fn test_write_dry_run() {
        let temp = tempdir().unwrap();
        let writer = SkillWriter::new(temp.path()).dry_run(true);
        let skill = create_test_skill();

        let report = writer.write_all(&[skill]).unwrap();
        assert_eq!(report.skills_created, 1);

        // File should NOT exist in dry run
        assert!(!temp.path().join("test-skill/SKILL.md").exists());
    }

    #[test]
    fn test_write_creates_files() {
        let temp = tempdir().unwrap();
        let writer = SkillWriter::new(temp.path());
        let skill = create_test_skill();

        let report = writer.write_all(&[skill]).unwrap();
        assert_eq!(report.skills_created, 1);

        // File should exist
        let skill_path = temp.path().join("test-skill/SKILL.md");
        assert!(skill_path.exists());

        // Validate content
        let content = fs::read_to_string(&skill_path).unwrap();
        assert!(content.contains("name: test-skill"));
        assert!(content.contains("## Implementation"));
    }

    #[test]
    fn test_validate_skill_file() {
        let temp = tempdir().unwrap();
        let skill_path = temp.path().join("test.md");

        // Valid skill
        fs::write(&skill_path, "---\nname: test\ndescription: test\n---\n# Test").unwrap();
        assert!(SkillWriter::validate_skill_file(&skill_path).is_ok());

        // Missing frontmatter
        fs::write(&skill_path, "# No frontmatter").unwrap();
        assert!(SkillWriter::validate_skill_file(&skill_path).is_err());

        // Missing name
        fs::write(&skill_path, "---\ndescription: test\n---\n").unwrap();
        assert!(SkillWriter::validate_skill_file(&skill_path).is_err());
    }
}
