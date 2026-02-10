//! Skill generation infrastructure
//!
//! This module provides generic components for AI-powered skill extraction
//! from source code, documentation, and API definitions.
//!
//! ## Architecture
//!
//! The skill generation pipeline consists of:
//!
//! 1. **Collectors** - Gather source material from a project
//!    - `CodeCollector` - Collects source code files
//!    - `DocCollector` - Collects documentation files
//!    - `ApiCollector` - Extracts API definitions from code
//!    - `SdkCollector` - Unified collector combining all above
//!
//! 2. **Extractor** - Uses AI to analyze sources and extract skills
//!    - `SkillExtractor` - Calls AI APIs via Provider trait
//!    - Accepts `PromptBuilder` for domain-specific prompt construction
//!    - Accepts `SkillValidator` for domain-specific validation
//!
//! 3. **Output** - Writes extracted skills to files
//!    - `SkillWriter` - Writes SKILL.md files and references
//!    - Accepts `OutputFormat` for domain-specific formatting
//!
//! ## Extensibility
//!
//! The module uses traits for domain customization:
//!
//! - `PromptBuilder` - Customize AI prompts for your domain
//! - `SkillValidator` - Add domain-specific validation rules
//! - `OutputFormat` - Customize SKILL.md output format
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use bxnode_bot::skillgen::{
//!     SdkCollector, SkillExtractor, SkillWriter,
//!     PromptBuilder, ExtractionContext, DefaultOutputFormat,
//! };
//!
//! // Implement domain-specific prompt builder
//! struct MyPromptBuilder;
//! impl PromptBuilder for MyPromptBuilder {
//!     fn system_prompt(&self) -> &str {
//!         "You are an expert in my domain..."
//!     }
//!     fn build_extraction_prompt(&self, sources: &CollectedSources, ctx: &ExtractionContext) -> String {
//!         // Build your prompt...
//!         String::new()
//!     }
//! }
//!
//! // Use the pipeline
//! let collector = SdkCollector::new();
//! let sources = collector.collect(path)?;
//!
//! let extractor = SkillExtractor::new(provider_registry);
//! let skills = extractor.extract_skills(&sources, &MyPromptBuilder, &context, None).await?;
//!
//! let writer = SkillWriter::new(output_dir);
//! writer.write_all(&skills)?;
//! ```

mod collector;
mod extractor;
mod output;
mod traits;
mod types;

// Re-export collector types
pub use collector::{
    ApiCollector, CodeCollector, CodeCollectorConfig, DocCollector, DocCollectorConfig,
    SdkCollector,
};

// Re-export extractor types
pub use extractor::{ExtractionResult, ExtractorConfig, SkillExtractor};

// Re-export output types
pub use output::SkillWriter;

// Re-export traits
pub use traits::{
    DefaultOutputFormat, DefaultSkillValidator, ExtractionContext, OutputFormat, PromptBuilder,
    SkillValidator,
};

// Re-export types
pub use types::{
    ApiDef, ApiKind, CodeExample, CollectedSources, DocFile, DocType, ExtractedSkill,
    GenerationReport, ImplementationGuide, ImplementationStep, Language, ReferenceFile,
    SkillgenConfig, SourceFile, ValidationReport,
};
