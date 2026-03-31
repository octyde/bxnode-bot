//! App loader - Parse APP.md files and scan directories

use std::collections::HashMap;
use std::path::Path;

use crate::apps::{
    AppDefinition, AppInfo, AppInput, AppMetadata, AppPhase, MetaField, OutputFields, SelectOption,
};

/// App loader for parsing APP.md files and scanning directories
pub struct AppLoader;

impl AppLoader {
    /// Scan a directory for app folders containing APP.md files
    pub fn scan_directory(path: &Path) -> anyhow::Result<Vec<AppDefinition>> {
        let mut apps = Vec::new();

        if !path.exists() {
            tracing::debug!("App directory does not exist: {}", path.display());
            return Ok(apps);
        }

        if !path.is_dir() {
            anyhow::bail!("Path is not a directory: {}", path.display());
        }

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let app_dir = entry.path();

            if app_dir.is_dir() {
                let app_md = app_dir.join("APP.md");
                if app_md.exists() {
                    match Self::parse_app(&app_md) {
                        Ok(app_def) => {
                            tracing::debug!(
                                "Discovered app: {} at {}",
                                app_def.metadata.name,
                                app_dir.display()
                            );
                            apps.push(app_def);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to parse APP.md at {}: {}",
                                app_md.display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        Ok(apps)
    }

    /// Parse an APP.md file into a full AppDefinition
    pub fn parse_app(path: &Path) -> anyhow::Result<AppDefinition> {
        let content = std::fs::read_to_string(path)?;
        let (metadata, body) = Self::parse_frontmatter(&content)?;

        let app_dir = path.parent().ok_or_else(|| {
            anyhow::anyhow!("APP.md must be in an app directory")
        })?;

        let inputs = Self::parse_inputs(&body);
        let phases = Self::parse_phases(&body);

        Ok(AppDefinition {
            metadata,
            source_path: app_dir.display().to_string(),
            inputs,
            phases,
        })
    }

    /// Parse YAML frontmatter from markdown content
    fn parse_frontmatter(content: &str) -> anyhow::Result<(AppMetadata, String)> {
        let content = content.trim();

        if !content.starts_with("---") {
            anyhow::bail!("APP.md must start with YAML frontmatter (---)");
        }

        let rest = &content[3..];
        let end_idx = rest.find("\n---").ok_or_else(|| {
            anyhow::anyhow!("APP.md frontmatter must be closed with ---")
        })?;

        let yaml_content = rest[..end_idx].trim();
        let body = rest[end_idx + 4..].trim().to_string();

        let metadata: AppMetadata = serde_yaml::from_str(yaml_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse APP.md frontmatter: {}", e))?;

        Ok((metadata, body))
    }

    /// Parse the ## Inputs section from the markdown body
    fn parse_inputs(body: &str) -> Vec<AppInput> {
        let mut inputs = Vec::new();

        let inputs_section = match Self::extract_section(body, "Inputs") {
            Some(s) => s,
            None => return inputs,
        };

        // Split by ### headings
        let subsections = Self::split_subsections(&inputs_section);

        for (name, content) in subsections {
            let props = Self::parse_properties(&content);

            let mut options = Vec::new();
            if let Some(opts_str) = Self::extract_options_block(&content) {
                options = Self::parse_select_options(&opts_str);
            }

            inputs.push(AppInput {
                name,
                input_type: props.get("type").cloned().unwrap_or_else(|| "text".to_string()),
                label: props.get("label").cloned(),
                default: props.get("default").cloned(),
                placeholder: props.get("placeholder").cloned(),
                min: props.get("min").and_then(|v| v.parse().ok()),
                max: props.get("max").and_then(|v| v.parse().ok()),
                options,
            });
        }

        inputs
    }

    /// Parse the ## Phases section from the markdown body
    fn parse_phases(body: &str) -> Vec<AppPhase> {
        let mut phases = Vec::new();

        let phases_section = match Self::extract_section(body, "Phases") {
            Some(s) => s,
            None => return phases,
        };

        let subsections = Self::split_subsections(&phases_section);

        for (name, content) in subsections {
            let props = Self::parse_properties(&content);

            // Parse output-fields if present
            let output_fields = Self::parse_output_fields(&content);

            phases.push(AppPhase {
                name,
                label: props.get("label").cloned(),
                button: props.get("button").cloned(),
                prompt: props.get("prompt").cloned(),
                output: props.get("output").cloned(),
                output_fields,
                selectable: props.get("selectable").map(|v| v == "true"),
                select_prompt: props.get("select-prompt").cloned(),
                requires_notes: props.get("requires-notes").map(|v| v == "true"),
            });
        }

        phases
    }

    /// Extract a ## section by heading name
    fn extract_section(body: &str, heading: &str) -> Option<String> {
        let marker = format!("## {}", heading);
        let start = body.find(&marker)?;
        let after_marker = start + marker.len();
        let section_body = &body[after_marker..];

        // Find the next ## heading (end of this section)
        let end = section_body
            .find("\n## ")
            .map(|i| after_marker + i)
            .unwrap_or(body.len());

        Some(body[after_marker..end].to_string())
    }

    /// Split a section into ### subsections, returning (name, content) pairs
    fn split_subsections(section: &str) -> Vec<(String, String)> {
        let mut result = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_lines: Vec<String> = Vec::new();

        for line in section.lines() {
            if line.starts_with("### ") {
                // Save previous subsection
                if let Some(name) = current_name.take() {
                    result.push((name, current_lines.join("\n")));
                }
                current_name = Some(line[4..].trim().to_string());
                current_lines.clear();
            } else if current_name.is_some() {
                current_lines.push(line.to_string());
            }
        }

        // Save last subsection
        if let Some(name) = current_name {
            result.push((name, current_lines.join("\n")));
        }

        result
    }

    /// Parse `- key: value` properties from content, handling multi-line values (| block)
    /// Only parses top-level (non-indented) properties; skips nested blocks like output-fields, options, meta.
    fn parse_properties(content: &str) -> HashMap<String, String> {
        let mut props = HashMap::new();
        let mut current_key: Option<String> = None;
        let mut current_value_lines: Vec<String> = Vec::new();
        let mut in_multiline = false;
        let mut skip_block = false;

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines at the start, but keep them in multiline values
            if trimmed.is_empty() {
                if in_multiline {
                    current_value_lines.push(String::new());
                }
                continue;
            }

            // Track whether we're inside a nested block (output-fields, options, meta)
            // A nested block starts with a top-level `- key:` (no value) and includes
            // all subsequent indented lines until the next top-level `- key:`
            let is_top_level = !line.starts_with("  ") || line.starts_with("- ");
            let is_indented = line.starts_with("    ") || line.starts_with("  ");

            if skip_block {
                // Still inside a nested block — skip indented lines
                if is_indented && !trimmed.starts_with("- ") || (trimmed.starts_with("- ") && line.starts_with("  ")) {
                    continue;
                }
                // Back to top-level
                skip_block = false;
            }

            if in_multiline {
                // Check if this line starts a new top-level `- key:` property
                if trimmed.starts_with("- ") && trimmed.contains(": ") && !line.starts_with("    ") {
                    // Save the multiline value
                    if let Some(key) = current_key.take() {
                        let value = current_value_lines.join("\n").trim().to_string();
                        if !value.is_empty() {
                            props.insert(key, value);
                        }
                    }
                    in_multiline = false;
                    // Fall through to parse this line as a new property
                } else {
                    // Part of the multiline value - strip common indent
                    let stripped = if line.starts_with("    ") {
                        &line[4..]
                    } else {
                        line
                    };
                    current_value_lines.push(stripped.to_string());
                    continue;
                }
            }

            // Parse top-level `- key: value` lines
            if trimmed.starts_with("- ") {
                let kv = &trimmed[2..];
                if let Some(colon_pos) = kv.find(": ") {
                    let key = kv[..colon_pos].trim().to_string();
                    let value = kv[colon_pos + 2..].trim().to_string();

                    // Check for multiline indicator
                    if value == "|" {
                        current_key = Some(key);
                        current_value_lines.clear();
                        in_multiline = true;
                    } else {
                        props.insert(key, value);
                    }
                } else if kv.ends_with(':') {
                    // Block start (e.g., `- output-fields:`, `- options:`, `- meta:`)
                    // Skip all indented content under it
                    skip_block = true;
                    continue;
                }
            }
        }

        // Save last multiline value
        if let Some(key) = current_key {
            let value = current_value_lines.join("\n").trim().to_string();
            if !value.is_empty() {
                props.insert(key, value);
            }
        }

        props
    }

    /// Extract the options block from input content
    fn extract_options_block(content: &str) -> Option<String> {
        let marker = "- options:";
        let start = content.find(marker)?;
        let after = &content[start + marker.len()..];
        Some(after.to_string())
    }

    /// Parse select options from an options block
    fn parse_select_options(content: &str) -> Vec<SelectOption> {
        let mut options = Vec::new();
        let mut current_label: Option<String> = None;
        let mut current_value: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("- label: ") {
                // Save previous option
                if let (Some(label), Some(value)) = (current_label.take(), current_value.take()) {
                    options.push(SelectOption { label, value });
                }
                current_label = Some(trimmed[9..].trim().to_string());
            } else if trimmed.starts_with("value: ") {
                current_value = Some(trimmed[7..].trim().to_string());
            }
        }

        // Save last option
        if let (Some(label), Some(value)) = (current_label, current_value) {
            options.push(SelectOption { label, value });
        }

        options
    }

    /// Parse output-fields from phase content
    fn parse_output_fields(content: &str) -> Option<OutputFields> {
        let marker = "- output-fields:";
        let start = content.find(marker)?;
        let after = &content[start + marker.len()..];

        let mut title = None;
        let mut body = None;
        let mut subtitle = None;
        let mut badge = None;
        let mut footer_left = None;
        let mut footer_right = None;
        let mut tags = None;
        let mut meta = Vec::new();
        let mut in_meta = false;
        let mut meta_label: Option<String> = None;

        for line in after.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                continue;
            }

            // Stop if we hit a new top-level property (not indented enough)
            if trimmed.starts_with("- ") && !trimmed.starts_with("- label:") && !trimmed.starts_with("- title:") && !line.starts_with("    ") {
                // Check if this is a new phase-level property
                if !line.starts_with("        ") && !line.starts_with("      ") {
                    break;
                }
            }

            if in_meta {
                if trimmed.starts_with("- label: ") {
                    // Save previous meta field
                    if let Some(label) = meta_label.take() {
                        // Label without field - skip
                        let _ = label;
                    }
                    meta_label = Some(trimmed[9..].trim().trim_matches('"').to_string());
                } else if trimmed.starts_with("field: ") {
                    if let Some(label) = meta_label.take() {
                        meta.push(MetaField {
                            label,
                            field: trimmed[7..].trim().to_string(),
                        });
                    }
                } else if trimmed.starts_with("- ") && !trimmed.starts_with("- label:") {
                    // New field outside meta
                    in_meta = false;
                    // Process this line below
                }
            }

            if !in_meta {
                if trimmed.starts_with("- title: ") {
                    title = Some(trimmed[9..].trim().to_string());
                } else if trimmed.starts_with("- body: ") {
                    body = Some(trimmed[8..].trim().to_string());
                } else if trimmed.starts_with("- subtitle: ") {
                    subtitle = Some(trimmed[12..].trim().to_string());
                } else if trimmed.starts_with("- badge: ") {
                    badge = Some(trimmed[9..].trim().to_string());
                } else if trimmed.starts_with("- footer-left: ") {
                    footer_left = Some(trimmed[15..].trim().to_string());
                } else if trimmed.starts_with("- footer-right: ") {
                    footer_right = Some(trimmed[16..].trim().to_string());
                } else if trimmed.starts_with("- tags: ") {
                    tags = Some(trimmed[8..].trim().to_string());
                } else if trimmed.starts_with("- meta:") {
                    in_meta = true;
                }
            }
        }

        Some(OutputFields {
            title,
            body,
            subtitle,
            badge,
            footer_left,
            footer_right,
            tags,
            meta,
        })
    }

    /// List app info (lightweight) from a directory scan
    pub fn list_app_info(path: &Path) -> anyhow::Result<Vec<AppInfo>> {
        let apps = Self::scan_directory(path)?;
        Ok(apps.iter().map(AppInfo::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_news_digest() {
        let app_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("apps/news-digest/APP.md");
        if !app_dir.exists() {
            return; // Skip if test files not present
        }

        let app = AppLoader::parse_app(&app_dir).unwrap();
        assert_eq!(app.metadata.name, "news-digest");
        assert_eq!(app.metadata.category, Some("Productivity".to_string()));
        assert_eq!(app.inputs.len(), 1);
        assert_eq!(app.inputs[0].name, "topics");
        assert_eq!(app.inputs[0].default, Some("AI, IOT".to_string()));
        assert_eq!(app.phases.len(), 1);
        assert_eq!(app.phases[0].name, "fetch");
        assert!(app.phases[0].prompt.as_ref().unwrap().contains("{{topics}}"));
        assert_eq!(app.phases[0].output, Some("cards".to_string()));
        assert!(app.phases[0].output_fields.is_some());
    }

    #[test]
    fn test_parse_money_maker() {
        let app_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("apps/money-maker/APP.md");
        if !app_dir.exists() {
            return;
        }

        let app = AppLoader::parse_app(&app_dir).unwrap();
        assert_eq!(app.metadata.name, "money-maker");
        assert_eq!(app.inputs.len(), 2);
        assert_eq!(app.inputs[1].name, "currency");
        assert_eq!(app.inputs[1].input_type, "select");
        assert!(app.inputs[1].options.len() >= 5);
        assert_eq!(app.phases.len(), 4);
        assert_eq!(app.phases[1].selectable, Some(true));
        assert_eq!(app.phases[3].requires_notes, Some(true));
    }
}
