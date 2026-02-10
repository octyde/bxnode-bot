//! Core types for SDK skill generation
//!
//! These types are generic and can be used across different domains.
//! Domain-specific fields are stored in the `extensions` field.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Collected source material from a project/SDK
#[derive(Debug, Clone, Default)]
pub struct CollectedSources {
    /// Code files (examples, libraries)
    pub code_files: Vec<SourceFile>,

    /// Documentation files
    pub doc_files: Vec<DocFile>,

    /// API definitions (from headers, schemas, etc.)
    pub api_defs: Vec<ApiDef>,

    /// Total bytes collected
    pub total_bytes: usize,

    /// Collection timestamp
    pub collected_at: Option<DateTime<Utc>>,
}

impl CollectedSources {
    /// Create empty collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if any sources were collected
    pub fn is_empty(&self) -> bool {
        self.code_files.is_empty() && self.doc_files.is_empty() && self.api_defs.is_empty()
    }

    /// Get total file count
    pub fn file_count(&self) -> usize {
        self.code_files.len() + self.doc_files.len()
    }

    /// Summarize for logging
    pub fn summary(&self) -> String {
        format!(
            "{} code files, {} doc files, {} API defs ({} bytes)",
            self.code_files.len(),
            self.doc_files.len(),
            self.api_defs.len(),
            self.total_bytes
        )
    }
}

/// A source code file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    /// Relative path from project root
    pub path: PathBuf,

    /// File content
    pub content: String,

    /// Detected language
    pub language: Language,

    /// File size in bytes
    pub size_bytes: usize,
}

/// A documentation file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocFile {
    /// Relative path from project root
    pub path: PathBuf,

    /// File content
    pub content: String,

    /// Documentation type
    pub doc_type: DocType,

    /// File size in bytes
    pub size_bytes: usize,
}

/// API definition extracted from source files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDef {
    /// Function/type/endpoint name
    pub name: String,

    /// API kind (function, struct, endpoint, etc.)
    pub kind: ApiKind,

    /// Full signature or definition
    pub signature: String,

    /// Associated comments/documentation
    pub doc_comment: Option<String>,

    /// Source file path
    pub source_file: PathBuf,

    /// Line number
    pub line_number: usize,
}

/// Programming language
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    C,
    Cpp,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Assembly,
    Linker,
    Make,
    Cmake,
    Yaml,
    Json,
    Toml,
    Sql,
    Unknown,
}

impl Language {
    /// Detect language from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "c" | "h" => Self::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Self::Cpp,
            "rs" => Self::Rust,
            "py" => Self::Python,
            "js" | "mjs" | "cjs" => Self::JavaScript,
            "ts" | "tsx" => Self::TypeScript,
            "go" => Self::Go,
            "java" => Self::Java,
            "s" | "asm" => Self::Assembly,
            "ld" | "x" => Self::Linker,
            "mk" | "makefile" => Self::Make,
            "cmake" => Self::Cmake,
            "yaml" | "yml" => Self::Yaml,
            "json" => Self::Json,
            "toml" => Self::Toml,
            "sql" => Self::Sql,
            _ => Self::Unknown,
        }
    }

    /// Get language name for display/code blocks
    pub fn name(&self) -> &'static str {
        match self {
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Go => "go",
            Self::Java => "java",
            Self::Assembly => "asm",
            Self::Linker => "linker",
            Self::Make => "makefile",
            Self::Cmake => "cmake",
            Self::Yaml => "yaml",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Sql => "sql",
            Self::Unknown => "text",
        }
    }
}

/// Documentation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocType {
    Readme,
    Markdown,
    Text,
    Html,
    Rst,
    AsciiDoc,
    Unknown,
}

impl DocType {
    /// Detect doc type from filename/extension
    pub fn from_path(path: &std::path::Path) -> Self {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        if filename.starts_with("readme") {
            return Self::Readme;
        }

        match path.extension().and_then(|e| e.to_str()) {
            Some("md") | Some("markdown") => Self::Markdown,
            Some("txt") => Self::Text,
            Some("html") | Some("htm") => Self::Html,
            Some("rst") => Self::Rst,
            Some("adoc") | Some("asciidoc") => Self::AsciiDoc,
            _ => Self::Unknown,
        }
    }
}

/// API definition kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKind {
    Function,
    Method,
    Struct,
    Class,
    Enum,
    Typedef,
    Macro,
    Constant,
    Variable,
    Endpoint,
    Schema,
}

/// Skill extracted by AI from source material
///
/// Core fields are generic; domain-specific data goes in `extensions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedSkill {
    /// Skill name (lowercase-with-hyphens)
    pub name: String,

    /// Short description (max 500 chars)
    pub description: String,

    /// Target platform/domain
    pub platform: String,

    /// When to use this skill
    #[serde(default)]
    pub use_cases: Vec<String>,

    /// Step-by-step implementation guide
    pub implementation: ImplementationGuide,

    /// Related skill names
    #[serde(default)]
    pub related_skills: Vec<String>,

    /// Reference files to include
    #[serde(default)]
    pub references: Vec<ReferenceFile>,

    /// Extraction confidence (0.0 - 1.0)
    #[serde(default)]
    pub confidence: f64,

    /// Domain-specific extension data
    ///
    /// This field holds any domain-specific data that doesn't fit
    /// in the generic fields. For embedded systems, this might include
    /// "peripherals", "power_notes", "pitfalls", "prerequisites".
    #[serde(default, flatten)]
    pub extensions: serde_json::Value,
}

impl ExtractedSkill {
    /// Get a domain-specific extension field
    pub fn get_extension<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.extensions
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get a string array extension field
    pub fn get_string_array(&self, key: &str) -> Vec<String> {
        self.get_extension::<Vec<String>>(key).unwrap_or_default()
    }

    /// Set a domain-specific extension field
    pub fn set_extension<T: Serialize>(&mut self, key: &str, value: T) {
        if let Ok(json_value) = serde_json::to_value(value) {
            if let serde_json::Value::Object(ref mut map) = self.extensions {
                map.insert(key.to_string(), json_value);
            } else {
                let mut map = serde_json::Map::new();
                map.insert(key.to_string(), json_value);
                self.extensions = serde_json::Value::Object(map);
            }
        }
    }
}

/// Implementation guide with code examples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationGuide {
    /// Overview text
    #[serde(default)]
    pub overview: String,

    /// Implementation steps
    pub steps: Vec<ImplementationStep>,
}

impl Default for ImplementationGuide {
    fn default() -> Self {
        Self {
            overview: String::new(),
            steps: Vec::new(),
        }
    }
}

/// A single implementation step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationStep {
    /// Step title
    pub title: String,

    /// Step description
    #[serde(default)]
    pub description: String,

    /// Code example (if any)
    #[serde(default)]
    pub code: Option<CodeExample>,
}

/// Code example with language
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExample {
    /// Code content
    pub content: String,

    /// Programming language
    pub language: String,

    /// Explanation of the code
    #[serde(default)]
    pub explanation: String,
}

/// Reference file to include with skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceFile {
    /// Filename (e.g., "schema.json")
    pub name: String,

    /// File content
    pub content: String,

    /// Optional description
    #[serde(default)]
    pub description: String,
}

/// Configuration for skill generation
#[derive(Debug, Clone)]
pub struct SkillgenConfig {
    /// AI model to use (provider/model format)
    pub model: String,

    /// Target platform (e.g., "stm32f401", "nodejs-api")
    pub platform: String,

    /// Focus on specific areas
    pub focus: Option<Vec<String>>,

    /// Output directory for generated skills
    pub output_dir: PathBuf,

    /// Maximum number of skills to generate
    pub max_skills: Option<usize>,

    /// Include raw examples as references
    pub include_raw_examples: bool,

    /// Enable interactive mode
    pub interactive: bool,

    /// Dry run (don't write files)
    pub dry_run: bool,
}

impl Default for SkillgenConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-3-sonnet".to_string(),
            platform: "generic".to_string(),
            focus: None,
            output_dir: PathBuf::from("skills"),
            max_skills: None,
            include_raw_examples: false,
            interactive: false,
            dry_run: false,
        }
    }
}

/// Report from skill generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationReport {
    /// Number of skills created
    pub skills_created: usize,

    /// Skill names created
    pub skill_names: Vec<String>,

    /// Source files processed
    pub source_files_processed: usize,

    /// Total tokens used (if available)
    pub tokens_used: Option<u64>,

    /// Generation duration in milliseconds
    pub duration_ms: u64,

    /// Errors encountered
    pub errors: Vec<String>,
}

impl GenerationReport {
    /// Create empty report
    pub fn new() -> Self {
        Self {
            skills_created: 0,
            skill_names: Vec::new(),
            source_files_processed: 0,
            tokens_used: None,
            duration_ms: 0,
            errors: Vec::new(),
        }
    }
}

impl Default for GenerationReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Report from skill validation
#[derive(Debug, Default)]
pub struct ValidationReport {
    /// Number of valid skills
    pub valid_count: usize,

    /// Number of invalid skills
    pub invalid_count: usize,

    /// Valid skill names
    pub skill_names: Vec<String>,

    /// Validation errors
    pub errors: Vec<String>,
}

impl ValidationReport {
    /// Check if all skills are valid
    pub fn is_success(&self) -> bool {
        self.invalid_count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension() {
        assert_eq!(Language::from_extension("c"), Language::C);
        assert_eq!(Language::from_extension("h"), Language::C);
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("xyz"), Language::Unknown);
    }

    #[test]
    fn test_doc_type_from_path() {
        use std::path::Path;

        assert_eq!(DocType::from_path(Path::new("README.md")), DocType::Readme);
        assert_eq!(
            DocType::from_path(Path::new("docs/api.md")),
            DocType::Markdown
        );
        assert_eq!(DocType::from_path(Path::new("notes.txt")), DocType::Text);
    }

    #[test]
    fn test_collected_sources_summary() {
        let mut sources = CollectedSources::new();
        sources.code_files.push(SourceFile {
            path: PathBuf::from("test.c"),
            content: "int main() {}".to_string(),
            language: Language::C,
            size_bytes: 13,
        });
        sources.total_bytes = 13;

        assert!(!sources.is_empty());
        assert_eq!(sources.file_count(), 1);
        assert!(sources.summary().contains("1 code files"));
    }

    #[test]
    fn test_extracted_skill_extensions() {
        let mut skill = ExtractedSkill {
            name: "test-skill".to_string(),
            description: "Test".to_string(),
            platform: "test".to_string(),
            use_cases: vec![],
            implementation: ImplementationGuide::default(),
            related_skills: vec![],
            references: vec![],
            confidence: 0.9,
            extensions: serde_json::json!({}),
        };

        // Set extension
        skill.set_extension("peripherals", vec!["gpio", "uart"]);

        // Get extension
        let peripherals: Vec<String> = skill.get_string_array("peripherals");
        assert_eq!(peripherals, vec!["gpio", "uart"]);
    }
}
