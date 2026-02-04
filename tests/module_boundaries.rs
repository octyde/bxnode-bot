//! Module boundary tests
//!
//! These tests ensure that modules are imported via their public entrypoints
//! rather than reaching into internal submodules.

use std::fs;
use std::path::Path;

/// Patterns that indicate deep imports (bypassing mod.rs)
///
/// Note: Some deep imports are allowed for implementation purposes:
/// - Feature-gated channel implementations (telegram, discord, slack) are constructed
///   in gateway/mod.rs using their specific config types
/// - Plugin API may need access to tool internals for registration
const DISALLOWED_PATTERNS: &[&str] = &[
    // Provider internals should not be accessed outside providers module
    "crate::providers::registry::",
    // These are allowed in gateway/mod.rs for channel construction
    // "crate::channels::telegram::",
    // "crate::channels::discord::",
    // "crate::channels::slack::",
    // Agent internals - context and execution should be accessed via public API
    "crate::agent::context::",
    "crate::agent::execution::",
    // Tools are exposed publicly, but internal details shouldn't leak
    // "crate::agent::tools::",  // Allowed for plugin API
    // Gateway internals
    "crate::gateway::http::",
    "crate::gateway::ws::",
    "crate::gateway::protocol::",
    // Session internals
    "crate::session::manager::",
    // Config test module
    "crate::config::tests::",
];

/// Files/directories to exclude from scanning
const EXCLUDED_PATHS: &[&str] = &[
    "target",
    "tests/module_boundaries.rs", // This file itself
    ".git",
];

/// Scan a file for disallowed import patterns
fn check_file_imports(path: &Path) -> Vec<(String, usize, String)> {
    let mut violations = Vec::new();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return violations,
    };

    for (line_num, line) in content.lines().enumerate() {
        // Skip comments
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        for pattern in DISALLOWED_PATTERNS {
            if line.contains(pattern) {
                violations.push((
                    path.display().to_string(),
                    line_num + 1,
                    pattern.to_string(),
                ));
            }
        }
    }

    violations
}

/// Recursively scan directory for Rust files
fn scan_directory(dir: &Path) -> Vec<(String, usize, String)> {
    let mut violations = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return violations,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let path_str = path.display().to_string();

        // Skip excluded paths
        if EXCLUDED_PATHS.iter().any(|ex| path_str.contains(ex)) {
            continue;
        }

        if path.is_dir() {
            violations.extend(scan_directory(&path));
        } else if path.extension().map_or(false, |ext| ext == "rs") {
            violations.extend(check_file_imports(&path));
        }
    }

    violations
}

#[test]
fn no_deep_imports_outside_modules() {
    let src_dir = Path::new("src");

    // Skip if src doesn't exist (running from wrong directory)
    if !src_dir.exists() {
        return;
    }

    let violations = scan_directory(src_dir);

    if !violations.is_empty() {
        let mut message = String::from("\nModule boundary violations found:\n\n");
        for (file, line, pattern) in &violations {
            message.push_str(&format!("  {}:{} - deep import: {}\n", file, line, pattern));
        }
        message.push_str("\nUse module-level imports instead:\n");
        message.push_str("  - use crate::providers::ProviderRegistry; (not crate::providers::registry::...)\n");
        message.push_str("  - use crate::channels::ChannelRegistry; (not crate::channels::registry::...)\n");
        message.push_str("\nSee docs/module-map.md for import guidelines.\n");

        panic!("{}", message);
    }
}

#[test]
fn all_modules_have_mod_rs() {
    let required_modules = [
        "src/agent/mod.rs",
        "src/channels/mod.rs",
        "src/cli/mod.rs",
        "src/config/mod.rs",
        "src/gateway/mod.rs",
        "src/plugins/mod.rs",
        "src/providers/mod.rs",
        "src/session/mod.rs",
    ];

    let mut missing = Vec::new();

    for module in required_modules {
        if !Path::new(module).exists() {
            missing.push(module);
        }
    }

    if !missing.is_empty() {
        panic!(
            "Missing module entrypoints:\n  {}\n\nEach module must have a mod.rs file.",
            missing.join("\n  ")
        );
    }
}

#[test]
fn lib_exports_all_modules() {
    let lib_path = Path::new("src/lib.rs");

    // Skip if lib.rs doesn't exist
    if !lib_path.exists() {
        return;
    }

    let content = fs::read_to_string(lib_path).expect("Failed to read lib.rs");

    let required_exports = [
        "pub mod agent",
        "pub mod channels",
        "pub mod cli",
        "pub mod config",
        "pub mod gateway",
        "pub mod plugins",
        "pub mod providers",
        "pub mod session",
    ];

    let mut missing = Vec::new();

    for export in required_exports {
        if !content.contains(export) {
            missing.push(export);
        }
    }

    if !missing.is_empty() {
        panic!(
            "lib.rs missing public module exports:\n  {}\n\nAll modules should be publicly exported.",
            missing.join("\n  ")
        );
    }
}
