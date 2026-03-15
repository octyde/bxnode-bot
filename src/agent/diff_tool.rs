//! Diff viewer tool - compare text content and show unified diffs.

use async_trait::async_trait;
use serde_json::json;
use similar::{ChangeTag, TextDiff};

use super::tools::Tool;

/// Diff viewer tool
pub struct DiffViewTool;

#[async_trait]
impl Tool for DiffViewTool {
    fn name(&self) -> &str {
        "diff_view"
    }

    fn description(&self) -> &str {
        "Compare two pieces of text and show the differences in unified diff format. Useful for reviewing changes before applying them."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "before": {
                    "type": "string",
                    "description": "The original text (before changes)"
                },
                "after": {
                    "type": "string",
                    "description": "The modified text (after changes)"
                },
                "label": {
                    "type": "string",
                    "description": "Optional label for the diff (e.g., filename)",
                    "default": "text"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines around changes (default: 3)",
                    "default": 3,
                    "minimum": 0,
                    "maximum": 10
                }
            },
            "required": ["before", "after"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let before = input
            .get("before")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'before' parameter"))?;
        let after = input
            .get("after")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'after' parameter"))?;
        let label = input
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("text");
        let context_lines = input
            .get("context_lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;

        if before == after {
            return Ok(json!({
                "has_changes": false,
                "diff": "(no differences)"
            })
            .to_string());
        }

        let diff = TextDiff::from_lines(before, after);
        let unified = diff.unified_diff();

        let mut output = String::new();
        output.push_str(&format!("--- a/{}\n+++ b/{}\n", label, label));

        for hunk in unified.iter_hunks() {
            // Hunk header
            output.push_str(&format!("{}\n", hunk.header()));

            for change in hunk.iter_changes() {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                output.push_str(sign);
                output.push_str(change.as_str().unwrap_or(""));
                if !change.as_str().unwrap_or("").ends_with('\n') {
                    output.push('\n');
                }
            }
        }

        // Count changes
        let mut additions = 0;
        let mut deletions = 0;
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => additions += 1,
                ChangeTag::Delete => deletions += 1,
                _ => {}
            }
        }

        let _ = context_lines; // Used by unified diff internally

        Ok(json!({
            "has_changes": true,
            "additions": additions,
            "deletions": deletions,
            "diff": output
        })
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diff_no_changes() {
        let tool = DiffViewTool;
        let result = tool
            .execute(json!({
                "before": "hello world",
                "after": "hello world"
            }))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["has_changes"], false);
    }

    #[tokio::test]
    async fn test_diff_with_changes() {
        let tool = DiffViewTool;
        let result = tool
            .execute(json!({
                "before": "line1\nline2\nline3\n",
                "after": "line1\nmodified\nline3\n",
                "label": "test.txt"
            }))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["has_changes"], true);
        assert_eq!(parsed["additions"], 1);
        assert_eq!(parsed["deletions"], 1);
        let diff_str = parsed["diff"].as_str().unwrap();
        assert!(diff_str.contains("-line2"));
        assert!(diff_str.contains("+modified"));
        assert!(diff_str.contains("--- a/test.txt"));
    }
}
