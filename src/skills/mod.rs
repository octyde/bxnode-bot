//! Skills module - OpenClaw/Agent Skills integration
//!
//! Skills provide Markdown-based instructions that guide agent behavior,
//! complementing the native plugin system which provides executable tools.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         BXNode Bot with Skills                          │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │  ┌─────────────────────┐        ┌─────────────────────┐                │
//! │  │   Plugin System     │        │   Skill System      │                │
//! │  │   (Native Rust)     │        │   (OpenClaw/MD)     │                │
//! │  │  - Tools (execute)  │        │  - Instructions     │                │
//! │  │  - Hooks (events)   │        │  - References       │                │
//! │  └──────────┬──────────┘        └──────────┬──────────┘                │
//! │             └──────────┬───────────────────┘                            │
//! │                        ▼                                                │
//! │  ┌─────────────────────────────────────────────────────────────────┐   │
//! │  │                     Agent Executor                               │   │
//! │  │  - Injects skill instructions into system prompt                 │   │
//! │  │  - Uses plugin tools for execution                               │   │
//! │  └─────────────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Skill Directory Structure
//!
//! ```text
//! skill-name/
//! ├── SKILL.md          # Required - YAML frontmatter + Markdown instructions
//! ├── scripts/          # Optional - executable code
//! ├── references/       # Optional - additional docs
//! └── assets/           # Optional - templates, images, data
//! ```
//!
//! # SKILL.md Format
//!
//! BXNode Bot supports the OpenClaw Agent Skills specification with extensions:
//!
//! ```yaml
//! ---
//! name: skill-name              # Required, lowercase + hyphens
//! description: What it does     # Required, max 1024 chars
//! # OpenClaw standard fields:
//! homepage: https://example.com # Optional - skill website
//! user-invocable: true          # Optional - expose as slash command (default: true)
//! disable-model-invocation: false # Optional - exclude from model prompt (default: false)
//! command-dispatch: tool        # Optional - bypass model, dispatch to tool directly
//! command-tool: tool-name       # Optional - tool to invoke with command-dispatch
//! # BXNode extensions:
//! license: MIT                  # Optional - license identifier
//! compatibility: ...            # Optional - environment requirements
//! allowed-tools: Bash Read      # Optional - pre-approved tools
//! ---
//!
//! # Markdown instructions for agents
//! Step-by-step instructions, examples, edge cases...
//! ```
//!
//! # Progressive Disclosure
//!
//! Skills use progressive disclosure to minimize context usage:
//! - Level 1: Only name + description loaded at startup (~100 tokens)
//! - Level 2: Full SKILL.md body loaded when skill is activated
//! - Level 3: Reference files loaded on demand

pub mod loader;
pub mod registry;
pub mod sync;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// Re-exports
pub use loader::SkillLoader;
pub use registry::SkillRegistry;
pub use sync::{SkillSyncSource, SkillSyncer, SyncReport};

/// Skill metadata from YAML frontmatter (Level 1 - always loaded)
///
/// Supports both OpenClaw Agent Skills specification and BXNode extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    // ========================================================================
    // Required fields (OpenClaw standard)
    // ========================================================================
    /// Unique skill identifier (lowercase letters, numbers, and hyphens)
    /// Must be 1-64 characters, must not start/end with hyphen
    pub name: String,

    /// Human-readable description (max 1024 characters)
    /// Should describe what the skill does and when to use it
    pub description: String,

    // ========================================================================
    // OpenClaw standard optional fields
    // ========================================================================
    /// Skill homepage URL (e.g., "https://github.com/user/skill")
    #[serde(default)]
    pub homepage: Option<String>,

    /// Whether this skill can be invoked by users as a slash command
    /// Default: true - skill is exposed as /skill-name command
    #[serde(default = "default_true", rename = "user-invocable")]
    pub user_invocable: bool,

    /// Whether to exclude this skill from the model's system prompt
    /// Default: false - skill instructions are included in model context
    /// Set to true for utility skills that only need slash command access
    #[serde(default, rename = "disable-model-invocation")]
    pub disable_model_invocation: bool,

    /// Command dispatch mode - bypasses the model and dispatches directly to a tool
    /// Set to "tool" to enable direct tool dispatch
    #[serde(default, rename = "command-dispatch")]
    pub command_dispatch: Option<String>,

    /// Tool name to invoke when command-dispatch is "tool"
    /// The skill arguments are passed to this tool
    #[serde(default, rename = "command-tool")]
    pub command_tool: Option<String>,

    // ========================================================================
    // BXNode extensions (backward compatible)
    // ========================================================================
    /// License identifier (e.g., "MIT", "Apache-2.0")
    #[serde(default)]
    pub license: Option<String>,

    /// Environment compatibility requirements
    /// E.g., "Requires git, docker" or "Designed for Claude Code"
    #[serde(default)]
    pub compatibility: Option<String>,

    /// Arbitrary key-value metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,

    /// Pre-approved tools this skill can use
    #[serde(default, rename = "allowed-tools")]
    pub allowed_tools: Vec<String>,
}

/// Default value for user_invocable (true)
fn default_true() -> bool {
    true
}

impl SkillMetadata {
    /// Validate the skill metadata
    pub fn validate(&self) -> Result<(), SkillValidationError> {
        // Validate name
        if self.name.is_empty() || self.name.len() > 64 {
            return Err(SkillValidationError::InvalidName(
                "Name must be 1-64 characters".to_string(),
            ));
        }

        if self.name.starts_with('-') || self.name.ends_with('-') {
            return Err(SkillValidationError::InvalidName(
                "Name must not start or end with hyphen".to_string(),
            ));
        }

        if self.name.contains("--") {
            return Err(SkillValidationError::InvalidName(
                "Name must not contain consecutive hyphens".to_string(),
            ));
        }

        // Check for valid characters (lowercase alphanumeric and hyphens)
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(SkillValidationError::InvalidName(
                "Name must contain only lowercase letters, numbers, and hyphens".to_string(),
            ));
        }

        // Validate description
        if self.description.is_empty() || self.description.len() > 1024 {
            return Err(SkillValidationError::InvalidDescription(
                "Description must be 1-1024 characters".to_string(),
            ));
        }

        // Validate compatibility length if provided
        if let Some(ref compat) = self.compatibility {
            if compat.len() > 500 {
                return Err(SkillValidationError::InvalidCompatibility(
                    "Compatibility must be max 500 characters".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Validation errors for skill metadata
#[derive(Debug, Clone, thiserror::Error)]
pub enum SkillValidationError {
    #[error("Invalid skill name: {0}")]
    InvalidName(String),

    #[error("Invalid skill description: {0}")]
    InvalidDescription(String),

    #[error("Invalid compatibility field: {0}")]
    InvalidCompatibility(String),
}

/// Skill with lazy-loaded content (progressive disclosure)
#[derive(Debug, Clone)]
pub struct SkillRef {
    /// Compact metadata (Level 1 - always loaded)
    pub metadata: SkillMetadata,

    /// Source directory path
    pub source_path: PathBuf,

    /// Whether full content is loaded (Level 2)
    pub is_activated: bool,

    /// Full markdown instructions (populated on activation)
    instructions: Option<String>,
}

impl SkillRef {
    /// Create a new skill reference with metadata only
    pub fn new(metadata: SkillMetadata, source_path: PathBuf) -> Self {
        Self {
            metadata,
            source_path,
            is_activated: false,
            instructions: None,
        }
    }

    /// Get the skill instructions (requires activation)
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// Set the skill instructions (called during activation)
    pub fn set_instructions(&mut self, instructions: String) {
        self.instructions = Some(instructions);
        self.is_activated = true;
    }

    /// Check if a reference file exists
    pub fn has_reference(&self, name: &str) -> bool {
        self.source_path.join("references").join(name).exists()
    }

    /// Check if a script exists
    pub fn has_script(&self, name: &str) -> bool {
        self.source_path.join("scripts").join(name).exists()
    }

    /// Get the path to an asset
    pub fn asset_path(&self, name: &str) -> PathBuf {
        self.source_path.join("assets").join(name)
    }
}

/// Full skill information for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    /// Skill metadata
    #[serde(flatten)]
    pub metadata: SkillMetadata,

    /// Source directory path
    pub source_path: String,

    /// Whether the skill is currently active
    pub is_active: bool,

    /// Whether full content is loaded
    pub is_activated: bool,

    /// Number of reference files
    pub reference_count: usize,

    /// Number of script files
    pub script_count: usize,
}

impl From<&SkillRef> for SkillInfo {
    fn from(skill: &SkillRef) -> Self {
        let reference_count = skill
            .source_path
            .join("references")
            .read_dir()
            .map(|r| r.count())
            .unwrap_or(0);

        let script_count = skill
            .source_path
            .join("scripts")
            .read_dir()
            .map(|r| r.count())
            .unwrap_or(0);

        Self {
            metadata: skill.metadata.clone(),
            source_path: skill.source_path.display().to_string(),
            is_active: false, // Set by registry
            is_activated: skill.is_activated,
            reference_count,
            script_count,
        }
    }
}
