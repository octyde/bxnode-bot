//! PDF analysis tool - extract and analyze PDF document content.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::json;

use super::tools::Tool;

/// Maximum PDF file size (32 MB)
const MAX_PDF_SIZE: u64 = 32 * 1024 * 1024;

/// Maximum pages to extract
const MAX_PAGES: usize = 50;

/// PDF analysis tool
pub struct PdfAnalyzeTool {
    workspace: PathBuf,
}

impl PdfAnalyzeTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

/// Resolve and validate a path within the workspace (same logic as coding_tools).
fn resolve_workspace_path(workspace: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    if relative.starts_with('/') || relative.starts_with('\\') {
        anyhow::bail!("Absolute paths are not allowed. Use paths relative to the workspace.");
    }

    let candidate = workspace.join(relative);

    let resolved = if candidate.exists() {
        candidate.canonicalize()?
    } else {
        anyhow::bail!("File not found: {}", relative);
    };

    let workspace_canonical = workspace.canonicalize()?;
    if !resolved.starts_with(&workspace_canonical) {
        anyhow::bail!("Path escapes workspace: {}", relative);
    }

    Ok(resolved)
}

/// Parse a page range specification like "1-5", "3", "1-3,7-9"
fn parse_page_range(spec: &str, total_pages: usize) -> Vec<usize> {
    let mut pages = Vec::new();

    for part in spec.split(',') {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            if let (Ok(s), Ok(e)) = (start.trim().parse::<usize>(), end.trim().parse::<usize>()) {
                for p in s..=e.min(total_pages) {
                    if p >= 1 && p <= total_pages && pages.len() < MAX_PAGES {
                        pages.push(p);
                    }
                }
            }
        } else if let Ok(p) = part.parse::<usize>() {
            if p >= 1 && p <= total_pages && pages.len() < MAX_PAGES {
                pages.push(p);
            }
        }
    }

    if pages.is_empty() {
        // Default: all pages up to MAX_PAGES
        pages = (1..=total_pages.min(MAX_PAGES)).collect();
    }

    pages
}

#[async_trait]
impl Tool for PdfAnalyzeTool {
    fn name(&self) -> &str {
        "pdf_analyze"
    }

    fn description(&self) -> &str {
        "Extract and analyze text content from a PDF file in the workspace. Returns the extracted text content, optionally filtered by page range."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path to the PDF file within the workspace"
                },
                "pages": {
                    "type": "string",
                    "description": "Page range to extract (e.g., '1-5', '3', '1-3,7-9'). Default: all pages."
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let rel_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let pages_spec = input.get("pages").and_then(|v| v.as_str());

        let path = resolve_workspace_path(&self.workspace, rel_path)?;

        if !path.is_file() {
            anyhow::bail!("Not a file: {}", rel_path);
        }

        // Check extension
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext.to_lowercase() != "pdf" {
            anyhow::bail!("Not a PDF file: {}", rel_path);
        }

        // Check file size
        let metadata = tokio::fs::metadata(&path).await?;
        if metadata.len() > MAX_PDF_SIZE {
            anyhow::bail!(
                "PDF too large ({} MB, max {} MB)",
                metadata.len() / (1024 * 1024),
                MAX_PDF_SIZE / (1024 * 1024)
            );
        }

        // Extract text using pdf-extract (blocking operation)
        let path_clone = path.clone();
        let text = tokio::task::spawn_blocking(move || {
            pdf_extract::extract_text(&path_clone)
        })
        .await?
        .map_err(|e| anyhow::anyhow!("Failed to extract PDF text: {}", e))?;

        // Split into pages (pdf-extract uses form feeds between pages)
        let all_pages: Vec<&str> = text.split('\u{0C}').collect();
        let total_pages = all_pages.len().max(1);

        // Apply page filter
        let selected_pages = if let Some(spec) = pages_spec {
            parse_page_range(spec, total_pages)
        } else {
            (1..=total_pages.min(MAX_PAGES)).collect()
        };

        let mut output = String::new();
        for &page_num in &selected_pages {
            if page_num > 0 && page_num <= all_pages.len() {
                let page_text = all_pages[page_num - 1].trim();
                if !page_text.is_empty() {
                    output.push_str(&format!("--- Page {} ---\n{}\n\n", page_num, page_text));
                }
            }
        }

        if output.is_empty() {
            output = "(No text content could be extracted from the PDF)".to_string();
        }

        Ok(json!({
            "file": rel_path,
            "total_pages": total_pages,
            "pages_extracted": selected_pages.len(),
            "content": output
        })
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_page_range_single() {
        assert_eq!(parse_page_range("3", 10), vec![3]);
    }

    #[test]
    fn test_parse_page_range_range() {
        assert_eq!(parse_page_range("2-5", 10), vec![2, 3, 4, 5]);
    }

    #[test]
    fn test_parse_page_range_multi() {
        assert_eq!(parse_page_range("1-3,7-9", 10), vec![1, 2, 3, 7, 8, 9]);
    }

    #[test]
    fn test_parse_page_range_clamp() {
        assert_eq!(parse_page_range("1-100", 5), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_parse_page_range_empty() {
        // Empty returns all pages
        let result = parse_page_range("", 3);
        assert_eq!(result, vec![1, 2, 3]);
    }
}
