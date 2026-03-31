//! Apps module - Markdown-defined application store
//!
//! Apps are self-contained AI-powered applications defined via APP.md files.
//! Each APP.md contains YAML frontmatter (metadata) and a structured markdown
//! body that defines inputs, phases, prompts, and output formats.
//!
//! # APP.md Format
//!
//! ```yaml
//! ---
//! name: app-name
//! description: What the app does
//! author: Author Name
//! version: "1.0.0"
//! category: Category
//! icon: lucide-icon-name
//! ---
//!
//! # App Title
//!
//! Description paragraph.
//!
//! ## Inputs
//!
//! ### input_name
//! - type: text | number | select
//! - label: Display Label
//! - default: default value
//! ...
//!
//! ## Phases
//!
//! ### phase_name
//! - label: Phase Label
//! - button: Button Text
//! - prompt: |
//!     AI prompt with {{variable}} substitution
//! - output: cards | checklist
//! - output-fields:
//!     - title: field_name
//!     ...
//! ```

pub mod loader;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use loader::AppLoader;

/// App metadata from YAML frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMetadata {
    /// Unique app identifier (lowercase + hyphens)
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Author name
    #[serde(default)]
    pub author: Option<String>,

    /// Version string
    #[serde(default)]
    pub version: Option<String>,

    /// Category for store listing
    #[serde(default)]
    pub category: Option<String>,

    /// Lucide icon name (e.g., "newspaper", "dollar-sign")
    #[serde(default)]
    pub icon: Option<String>,
}

/// Select option for select-type inputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

/// An input field definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInput {
    /// Input field name (the heading)
    pub name: String,

    /// Input type: text, number, select
    #[serde(alias = "type", default = "default_text")]
    pub input_type: String,

    /// Display label
    #[serde(default)]
    pub label: Option<String>,

    /// Default value
    #[serde(default)]
    pub default: Option<String>,

    /// Placeholder text (for text inputs)
    #[serde(default)]
    pub placeholder: Option<String>,

    /// Min value (for number inputs)
    #[serde(default)]
    pub min: Option<f64>,

    /// Max value (for number inputs)
    #[serde(default)]
    pub max: Option<f64>,

    /// Options (for select inputs)
    #[serde(default)]
    pub options: Vec<SelectOption>,
}

fn default_text() -> String {
    "text".to_string()
}

/// Output field mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaField {
    pub label: String,
    pub field: String,
}

/// Output fields configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFields {
    /// Field name for the card title
    #[serde(default)]
    pub title: Option<String>,

    /// Field name for the card body
    #[serde(default)]
    pub body: Option<String>,

    /// Field name for subtitle
    #[serde(default)]
    pub subtitle: Option<String>,

    /// Field name for badge
    #[serde(default)]
    pub badge: Option<String>,

    /// Field name for footer left
    #[serde(alias = "footer-left", default)]
    pub footer_left: Option<String>,

    /// Field name for footer right
    #[serde(alias = "footer-right", default)]
    pub footer_right: Option<String>,

    /// Field name for tags array
    #[serde(default)]
    pub tags: Option<String>,

    /// Key-value meta fields
    #[serde(default)]
    pub meta: Vec<MetaField>,
}

/// A phase definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPhase {
    /// Phase name (the heading)
    pub name: String,

    /// Display label
    #[serde(default)]
    pub label: Option<String>,

    /// Button text
    #[serde(default)]
    pub button: Option<String>,

    /// AI prompt template with {{variable}} substitution
    #[serde(default)]
    pub prompt: Option<String>,

    /// Output format: "cards" or "checklist"
    #[serde(default)]
    pub output: Option<String>,

    /// Output field mappings
    #[serde(alias = "output-fields", default)]
    pub output_fields: Option<OutputFields>,

    /// Whether items in this phase are selectable (to pick one for next phase)
    #[serde(default)]
    pub selectable: Option<bool>,

    /// Button text for selecting an item
    #[serde(alias = "select-prompt", default)]
    pub select_prompt: Option<String>,

    /// Whether this phase requires user notes
    #[serde(alias = "requires-notes", default)]
    pub requires_notes: Option<bool>,
}

/// Parsed app definition from APP.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDefinition {
    /// App metadata from frontmatter
    pub metadata: AppMetadata,

    /// Source directory path
    pub source_path: String,

    /// Input field definitions
    pub inputs: Vec<AppInput>,

    /// Phase definitions (ordered)
    pub phases: Vec<AppPhase>,
}

/// App info for listing (lightweight)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub source_path: String,
}

impl From<&AppDefinition> for AppInfo {
    fn from(def: &AppDefinition) -> Self {
        Self {
            name: def.metadata.name.clone(),
            description: def.metadata.description.clone(),
            author: def.metadata.author.clone(),
            version: def.metadata.version.clone(),
            category: def.metadata.category.clone(),
            icon: def.metadata.icon.clone(),
            source_path: def.source_path.clone(),
        }
    }
}
