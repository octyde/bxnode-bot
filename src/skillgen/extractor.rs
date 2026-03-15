//! AI-powered skill extractor using Provider trait
//!
//! This module extracts skills from collected sources using AI APIs.
//! It accepts a PromptBuilder trait object for domain-specific prompt construction.

use std::sync::Arc;

use anyhow::Context;
use tracing::{debug, info, warn};

use crate::providers::{CompletionRequest, Message, ProviderRegistry, Role};

use super::traits::{ExtractionContext, PromptBuilder, SkillValidator};
use super::types::{CollectedSources, ExtractedSkill};

/// Configuration for skill extraction
#[derive(Debug, Clone)]
pub struct ExtractorConfig {
    /// AI model to use (provider/model format)
    pub model: String,

    /// Temperature for generation (lower = more consistent)
    pub temperature: f32,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// Retry attempts on failure
    pub max_retries: usize,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-3-sonnet".to_string(),
            temperature: 0.3,
            max_tokens: 16000,
            max_retries: 2,
        }
    }
}

/// AI-powered skill extractor
///
/// This struct handles the AI API calls and JSON parsing.
/// Domain-specific prompt construction is delegated to PromptBuilder.
pub struct SkillExtractor {
    provider_registry: Arc<ProviderRegistry>,
    config: ExtractorConfig,
}

impl SkillExtractor {
    /// Create a new extractor with provider registry
    pub fn new(provider_registry: Arc<ProviderRegistry>) -> Self {
        Self {
            provider_registry,
            config: ExtractorConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(provider_registry: Arc<ProviderRegistry>, config: ExtractorConfig) -> Self {
        Self {
            provider_registry,
            config,
        }
    }

    /// Set the model to use
    pub fn set_model(&mut self, model: &str) {
        self.config.model = model.to_string();
    }

    /// Get current model
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Extract skills from collected sources using a prompt builder
    ///
    /// # Arguments
    /// * `sources` - Collected source material
    /// * `prompt_builder` - Domain-specific prompt construction
    /// * `context` - Extraction context (platform, focus areas)
    /// * `validator` - Optional validator for extracted skills
    ///
    /// # Returns
    /// Vector of extracted and validated skills
    pub async fn extract_skills(
        &self,
        sources: &CollectedSources,
        prompt_builder: &dyn PromptBuilder,
        context: &ExtractionContext,
        validator: Option<&dyn SkillValidator>,
    ) -> anyhow::Result<Vec<ExtractedSkill>> {
        info!(
            "Extracting skills using model: {} for platform: {}",
            self.config.model, context.platform
        );

        // Get provider for model
        let provider = self
            .provider_registry
            .get_for_model(&self.config.model)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Provider not found for model '{}'. Available providers: {:?}",
                    self.config.model,
                    self.provider_registry.provider_ids()
                )
            })?;

        // Build prompts using the domain-specific prompt builder
        let system_prompt = prompt_builder.system_prompt();
        let user_prompt = prompt_builder.build_extraction_prompt(sources, context);
        debug!("Built extraction prompt ({} chars)", user_prompt.len());

        // Create completion request
        let model_name = self
            .provider_registry
            .extract_model_name(&self.config.model);
        let request = CompletionRequest {
            model: model_name.clone(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: system_prompt.to_string(),
                },
                Message {
                    role: Role::User,
                    content: user_prompt,
                },
            ],
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_tokens),
            stream: false,
            stop: Vec::new(),
            tools: vec![],
        };

        // Call AI with retries
        let mut last_error = None;
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                info!("Retry attempt {} of {}", attempt, self.config.max_retries);
            }

            match provider.complete(request.clone()).await {
                Ok(response) => {
                    debug!("Received response ({} chars)", response.content.len());

                    // Parse JSON response
                    match self.parse_skills_response(&response.content) {
                        Ok(skills) => {
                            // Validate if validator provided
                            let valid_skills = if let Some(v) = validator {
                                skills
                                    .into_iter()
                                    .filter(|s| match v.validate(s) {
                                        Ok(()) => true,
                                        Err(errors) => {
                                            warn!(
                                                "Skill '{}' failed validation: {:?}",
                                                s.name, errors
                                            );
                                            false
                                        }
                                    })
                                    .collect()
                            } else {
                                skills
                            };

                            info!("Extracted {} skills", valid_skills.len());
                            return Ok(valid_skills);
                        }
                        Err(e) => {
                            warn!("Failed to parse AI response: {}", e);
                            last_error = Some(e);
                            // Continue to retry
                        }
                    }
                }
                Err(e) => {
                    warn!("AI request failed: {}", e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Extraction failed")))
    }

    /// Parse skills from AI response
    fn parse_skills_response(&self, response: &str) -> anyhow::Result<Vec<ExtractedSkill>> {
        // Try to find JSON array in response
        let json_str = self.extract_json_array(response)?;

        // Parse JSON
        let skills: Vec<ExtractedSkill> =
            serde_json::from_str(&json_str).context("Failed to parse skills JSON")?;

        if skills.is_empty() {
            anyhow::bail!("No skills extracted from response");
        }

        Ok(skills)
    }

    /// Extract JSON array from response (handles markdown code blocks)
    fn extract_json_array(&self, response: &str) -> anyhow::Result<String> {
        let trimmed = response.trim();

        // Try direct parse first
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return Ok(trimmed.to_string());
        }

        // Try to find JSON in code block
        if let Some(start) = trimmed.find("```json") {
            let after_marker = &trimmed[start + 7..];
            if let Some(end) = after_marker.find("```") {
                let json_content = after_marker[..end].trim();
                if json_content.starts_with('[') {
                    return Ok(json_content.to_string());
                }
            }
        }

        // Try to find plain code block
        if let Some(start) = trimmed.find("```") {
            let after_marker = &trimmed[start + 3..];
            // Skip language identifier if present
            let content_start = after_marker.find('\n').unwrap_or(0) + 1;
            let after_lang = &after_marker[content_start..];
            if let Some(end) = after_lang.find("```") {
                let json_content = after_lang[..end].trim();
                if json_content.starts_with('[') {
                    return Ok(json_content.to_string());
                }
            }
        }

        // Try to find array anywhere in response
        if let Some(start) = trimmed.find('[') {
            if let Some(end) = trimmed.rfind(']') {
                if end > start {
                    return Ok(trimmed[start..=end].to_string());
                }
            }
        }

        anyhow::bail!("No JSON array found in response")
    }

    /// Get list of available models
    pub fn available_models(&self) -> Vec<String> {
        self.provider_registry
            .all_models_prefixed()
            .into_iter()
            .map(|m| m.id)
            .collect()
    }

    /// Check if a model is available
    pub fn is_model_available(&self, model: &str) -> bool {
        self.provider_registry.get_for_model(model).is_some()
    }
}

/// Result of extraction with metadata
#[derive(Debug)]
pub struct ExtractionResult {
    /// Extracted skills
    pub skills: Vec<ExtractedSkill>,

    /// Model used
    pub model: String,

    /// Tokens used (if available)
    pub tokens_used: Option<u64>,

    /// Duration in milliseconds
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_array_direct() {
        let extractor = SkillExtractor::new(Arc::new(ProviderRegistry::new()));

        let response = r#"[{"name": "test-skill"}]"#;
        let result = extractor.extract_json_array(response);
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with('['));
    }

    #[test]
    fn test_extract_json_array_code_block() {
        let extractor = SkillExtractor::new(Arc::new(ProviderRegistry::new()));

        let response = r#"Here are the skills:

```json
[{"name": "test-skill"}]
```

Done!"#;
        let result = extractor.extract_json_array(response);
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_json_array_embedded() {
        let extractor = SkillExtractor::new(Arc::new(ProviderRegistry::new()));

        let response = r#"Here are the extracted skills:
[{"name": "skill-1"}, {"name": "skill-2"}]
Hope this helps!"#;
        let result = extractor.extract_json_array(response);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("skill-1"));
        assert!(json.contains("skill-2"));
    }
}
