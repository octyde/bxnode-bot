//! SecretRef resolution for config values.
//!
//! Supports three reference formats in config string values:
//! - `${env:VAR}` — resolve from environment variable
//! - `${file:/path/to/file}` — read file contents (trimmed)
//! - `${file:/path/to/file.json#/pointer}` — JSON pointer extraction
//! - `${exec:command args}` — run command, capture stdout (trimmed)

use anyhow::{Context, Result};
use regex::Regex;
use std::sync::LazyLock;

/// Regex matching `${type:arg}` patterns in strings
static REF_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{(env|file|exec):([^}]+)\}").unwrap());

/// Resolve all `${type:arg}` references in a serde_json::Value tree (recursive).
pub fn resolve_all(value: &mut serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::String(s) => {
            if s.contains("${") {
                *s = resolve_refs_in_string(s)?;
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                resolve_all(v)?;
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                resolve_all(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Replace all `${...}` references within a single string.
/// Supports multiple refs in one string: `"Bearer ${env:TOKEN}"`.
fn resolve_refs_in_string(s: &str) -> Result<String> {
    let mut result = s.to_string();
    // Collect matches first to avoid borrow issues during replacement
    let matches: Vec<(String, String, String)> = REF_PATTERN
        .captures_iter(s)
        .map(|cap| {
            (
                cap[0].to_string(),    // full match
                cap[1].to_string(),    // type
                cap[2].to_string(),    // arg
            )
        })
        .collect();

    for (full_match, ref_type, arg) in matches {
        let resolved = resolve_single(&ref_type, &arg)
            .with_context(|| format!("Failed to resolve {}", full_match))?;
        result = result.replace(&full_match, &resolved);
    }
    Ok(result)
}

/// Resolve a single secret reference by type.
fn resolve_single(ref_type: &str, arg: &str) -> Result<String> {
    match ref_type {
        "env" => resolve_env(arg),
        "file" => resolve_file(arg),
        "exec" => resolve_exec(arg),
        _ => anyhow::bail!("Unknown secret ref type: {}", ref_type),
    }
}

/// Resolve `${env:VAR}` — read from environment variable.
fn resolve_env(var_name: &str) -> Result<String> {
    std::env::var(var_name)
        .with_context(|| format!("Environment variable '{}' not set", var_name))
}

/// Resolve `${file:/path}` or `${file:/path/to/file.json#/pointer}`.
fn resolve_file(arg: &str) -> Result<String> {
    // Check for JSON pointer: path#/pointer
    if let Some((path, pointer)) = arg.split_once('#') {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Cannot read file: {}", path))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .with_context(|| format!("Cannot parse JSON from: {}", path))?;
        let value = json
            .pointer(pointer)
            .ok_or_else(|| anyhow::anyhow!("JSON pointer '{}' not found in {}", pointer, path))?;
        match value {
            serde_json::Value::String(s) => Ok(s.clone()),
            other => Ok(other.to_string()),
        }
    } else {
        let content = std::fs::read_to_string(arg)
            .with_context(|| format!("Cannot read file: {}", arg))?;
        Ok(content.trim().to_string())
    }
}

/// Resolve `${exec:command args}` — run via sh -c, capture stdout.
fn resolve_exec(cmd: &str) -> Result<String> {
    tracing::warn!(
        "Resolving exec secret ref: {}",
        if cmd.len() > 40 {
            format!("{}...", &cmd[..40])
        } else {
            cmd.to_string()
        }
    );

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .with_context(|| format!("Failed to execute: {}", cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Command exited with {}: {}",
            output.status,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("Command output is not valid UTF-8")?;
    Ok(stdout.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn test_resolve_env() {
        std::env::set_var("BXNODE_TEST_SECRET", "my-secret-value");
        let result = resolve_env("BXNODE_TEST_SECRET").unwrap();
        assert_eq!(result, "my-secret-value");
        std::env::remove_var("BXNODE_TEST_SECRET");
    }

    #[test]
    fn test_resolve_env_missing() {
        let result = resolve_env("BXNODE_NONEXISTENT_VAR_12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_file_plain() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "file-secret-value").unwrap();
        let result = resolve_file(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, "file-secret-value");
    }

    #[test]
    fn test_resolve_file_json_pointer() {
        let mut tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
        write!(tmp, r#"{{"api":{{"keys":{{"claude":"sk-abc123"}}}}}}"#).unwrap();
        let arg = format!("{}#/api/keys/claude", tmp.path().display());
        let result = resolve_file(&arg).unwrap();
        assert_eq!(result, "sk-abc123");
    }

    #[test]
    fn test_resolve_file_json_pointer_not_found() {
        let mut tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
        write!(tmp, r#"{{"a": 1}}"#).unwrap();
        let arg = format!("{}#/nonexistent", tmp.path().display());
        let result = resolve_file(&arg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_resolve_exec() {
        let result = resolve_exec("echo hello-from-exec").unwrap();
        assert_eq!(result, "hello-from-exec");
    }

    #[test]
    fn test_resolve_exec_failure() {
        let result = resolve_exec("exit 1");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_refs_in_string_single() {
        std::env::set_var("BXNODE_TEST_TOKEN", "tok-123");
        let result = resolve_refs_in_string("Bearer ${env:BXNODE_TEST_TOKEN}").unwrap();
        assert_eq!(result, "Bearer tok-123");
        std::env::remove_var("BXNODE_TEST_TOKEN");
    }

    #[test]
    fn test_resolve_refs_in_string_multiple() {
        std::env::set_var("BXNODE_HOST", "example.com");
        std::env::set_var("BXNODE_PORT", "8080");
        let result =
            resolve_refs_in_string("http://${env:BXNODE_HOST}:${env:BXNODE_PORT}/api").unwrap();
        assert_eq!(result, "http://example.com:8080/api");
        std::env::remove_var("BXNODE_HOST");
        std::env::remove_var("BXNODE_PORT");
    }

    #[test]
    fn test_resolve_refs_no_refs() {
        let result = resolve_refs_in_string("plain string without refs").unwrap();
        assert_eq!(result, "plain string without refs");
    }

    #[test]
    fn test_resolve_all_nested() {
        std::env::set_var("BXNODE_NESTED_KEY", "resolved-value");
        let mut value = json!({
            "level1": {
                "level2": "${env:BXNODE_NESTED_KEY}",
                "array": ["${env:BXNODE_NESTED_KEY}", "plain"],
                "number": 42
            }
        });
        resolve_all(&mut value).unwrap();
        assert_eq!(value["level1"]["level2"], "resolved-value");
        assert_eq!(value["level1"]["array"][0], "resolved-value");
        assert_eq!(value["level1"]["array"][1], "plain");
        assert_eq!(value["level1"]["number"], 42);
        std::env::remove_var("BXNODE_NESTED_KEY");
    }
}
