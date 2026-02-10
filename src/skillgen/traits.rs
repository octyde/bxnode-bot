//! Traits for extensible skill generation
//!
//! These traits allow domain-specific customization of the skill generation pipeline.

use super::types::{CollectedSources, ExtractedSkill};

/// Context for skill extraction
#[derive(Debug, Clone, Default)]
pub struct ExtractionContext {
    /// Target platform/domain identifier
    pub platform: String,

    /// Focus areas to prioritize during extraction
    pub focus: Option<Vec<String>>,

    /// Maximum number of skills to extract
    pub max_skills: Option<usize>,

    /// Additional context as key-value pairs
    pub extra: std::collections::HashMap<String, String>,
}

impl ExtractionContext {
    /// Create a new extraction context
    pub fn new(platform: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
            ..Default::default()
        }
    }

    /// Set focus areas
    pub fn with_focus(mut self, focus: Vec<String>) -> Self {
        self.focus = Some(focus);
        self
    }

    /// Set maximum skills
    pub fn with_max_skills(mut self, max: usize) -> Self {
        self.max_skills = Some(max);
        self
    }
}

/// Builds domain-specific prompts for skill extraction
///
/// Implement this trait to customize how AI prompts are constructed
/// for your specific domain (embedded systems, web APIs, databases, etc.)
pub trait PromptBuilder: Send + Sync {
    /// System prompt establishing AI's domain expertise
    ///
    /// This should describe the AI's role and expertise for the domain.
    fn system_prompt(&self) -> &str;

    /// Build the user prompt from collected sources
    ///
    /// This constructs the main extraction prompt including source code,
    /// documentation, and any domain-specific instructions.
    fn build_extraction_prompt(
        &self,
        sources: &CollectedSources,
        context: &ExtractionContext,
    ) -> String;

    /// Optional: Additional focus areas for extraction
    fn focus_areas(&self) -> Option<&[String]> {
        None
    }
}

/// Validates extracted skills according to domain rules
///
/// Implement this trait to add domain-specific validation beyond
/// the basic structural checks.
pub trait SkillValidator: Send + Sync {
    /// Validate a single skill, returning errors if invalid
    fn validate(&self, skill: &ExtractedSkill) -> Result<(), Vec<String>>;

    /// Check if skill name follows conventions
    ///
    /// Default implementation requires lowercase with hyphens, max 64 chars.
    fn validate_name(&self, name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }
}

/// Default validator with generic rules only
#[derive(Debug, Clone, Default)]
pub struct DefaultSkillValidator;

impl SkillValidator for DefaultSkillValidator {
    fn validate(&self, skill: &ExtractedSkill) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if !self.validate_name(&skill.name) {
            errors.push(format!("Invalid skill name: {}", skill.name));
        }

        if skill.description.is_empty() {
            errors.push("Missing description".to_string());
        }

        if skill.description.len() > 500 {
            errors.push("Description too long (max 500 chars)".to_string());
        }

        if skill.implementation.steps.is_empty() {
            errors.push("No implementation steps".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Customizes skill output format
///
/// Implement this trait to customize how skills are formatted
/// in the output SKILL.md files.
pub trait OutputFormat: Send + Sync {
    /// Format skill as SKILL.md content
    ///
    /// Returns the complete markdown content for the skill file.
    fn format_skill(&self, skill: &ExtractedSkill) -> String;

    /// Additional frontmatter fields beyond the standard ones
    ///
    /// Returns key-value pairs to add to YAML frontmatter.
    fn extra_frontmatter(&self, _skill: &ExtractedSkill) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Additional sections to add after implementation
    ///
    /// Returns section title and content pairs.
    fn extra_sections(&self, _skill: &ExtractedSkill) -> Vec<(String, String)> {
        Vec::new()
    }
}

/// Default output format for SKILL.md files
#[derive(Debug, Clone)]
pub struct DefaultOutputFormat {
    /// AI model used for generation (included in metadata)
    pub model: String,
}

impl Default for DefaultOutputFormat {
    fn default() -> Self {
        Self {
            model: "unknown".to_string(),
        }
    }
}

impl OutputFormat for DefaultOutputFormat {
    fn format_skill(&self, skill: &ExtractedSkill) -> String {
        let mut md = String::new();

        // YAML frontmatter
        md.push_str("---\n");
        md.push_str(&format!("name: {}\n", skill.name));
        md.push_str(&format!("description: {}\n", escape_yaml(&skill.description)));
        md.push_str("user-invocable: true\n");

        // Extra frontmatter from trait
        for (key, value) in self.extra_frontmatter(skill) {
            md.push_str(&format!("{}: {}\n", key, value));
        }

        md.push_str("metadata:\n");
        md.push_str(&format!("  platform: {}\n", skill.platform));
        md.push_str(&format!("  ai_model: \"{}\"\n", self.model));
        md.push_str(&format!(
            "  generated_at: \"{}\"\n",
            chrono::Utc::now().to_rfc3339()
        ));

        if skill.confidence > 0.0 {
            md.push_str(&format!("  confidence: {:.2}\n", skill.confidence));
        }

        md.push_str("---\n\n");

        // Title
        md.push_str(&format!("# {}\n\n", title_case(&skill.name)));

        // Overview
        if !skill.implementation.overview.is_empty() {
            md.push_str("## Overview\n\n");
            md.push_str(&skill.implementation.overview);
            md.push_str("\n\n");
        }

        // When to Use
        if !skill.use_cases.is_empty() {
            md.push_str("## When to Use\n\n");
            for use_case in &skill.use_cases {
                md.push_str(&format!("- {}\n", use_case));
            }
            md.push_str("\n");
        }

        // Implementation
        md.push_str("## Implementation\n\n");
        for (i, step) in skill.implementation.steps.iter().enumerate() {
            md.push_str(&format!("### Step {}: {}\n\n", i + 1, step.title));

            if !step.description.is_empty() {
                md.push_str(&step.description);
                md.push_str("\n\n");
            }

            if let Some(ref code) = step.code {
                md.push_str(&format!("```{}\n", code.language));
                md.push_str(&code.content);
                if !code.content.ends_with('\n') {
                    md.push('\n');
                }
                md.push_str("```\n\n");

                if !code.explanation.is_empty() {
                    md.push_str(&code.explanation);
                    md.push_str("\n\n");
                }
            }
        }

        // Extra sections from trait
        for (title, content) in self.extra_sections(skill) {
            md.push_str(&format!("## {}\n\n", title));
            md.push_str(&content);
            md.push_str("\n\n");
        }

        // Related Skills
        if !skill.related_skills.is_empty() {
            md.push_str("## Related Skills\n\n");
            for related in &skill.related_skills {
                md.push_str(&format!("- `{}`\n", related));
            }
            md.push_str("\n");
        }

        // References
        if !skill.references.is_empty() {
            md.push_str("## Reference Files\n\n");
            for ref_file in &skill.references {
                md.push_str(&format!(
                    "- [`{}`](references/{})",
                    ref_file.name, ref_file.name
                ));
                if !ref_file.description.is_empty() {
                    md.push_str(&format!(": {}", ref_file.description));
                }
                md.push('\n');
            }
        }

        md
    }
}

/// Convert skill name to title case
fn title_case(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Escape string for YAML
fn escape_yaml(s: &str) -> String {
    if s.contains(':') || s.contains('#') || s.contains('\n') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_title_case() {
        assert_eq!(title_case("stm32-gpio-output"), "Stm32 Gpio Output");
        assert_eq!(title_case("simple"), "Simple");
        assert_eq!(title_case("api-v2"), "Api V2");
    }

    #[test]
    fn test_escape_yaml() {
        assert_eq!(escape_yaml("simple"), "simple");
        assert_eq!(escape_yaml("with: colon"), "\"with: colon\"");
        assert_eq!(escape_yaml("with \"quotes\""), "\"with \\\"quotes\\\"\"");
    }

    #[test]
    fn test_default_validator_valid_name() {
        let validator = DefaultSkillValidator;
        assert!(validator.validate_name("valid-skill-name"));
        assert!(validator.validate_name("skill123"));
        assert!(!validator.validate_name("INVALID"));
        assert!(!validator.validate_name("with spaces"));
        assert!(!validator.validate_name(""));
    }

    #[test]
    fn test_extraction_context_builder() {
        let ctx = ExtractionContext::new("stm32f401")
            .with_focus(vec!["gpio".to_string(), "uart".to_string()])
            .with_max_skills(10);

        assert_eq!(ctx.platform, "stm32f401");
        assert_eq!(ctx.focus, Some(vec!["gpio".to_string(), "uart".to_string()]));
        assert_eq!(ctx.max_skills, Some(10));
    }
}
