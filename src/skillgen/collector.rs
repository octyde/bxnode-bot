//! Source collectors - gather code, docs, and API definitions from projects
//!
//! These collectors are generic and can be customized for different domains.

use std::path::{Path, PathBuf};

use regex::Regex;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use super::types::{ApiDef, ApiKind, CollectedSources, DocFile, DocType, Language, SourceFile};

/// Configuration for code collection
#[derive(Debug, Clone)]
pub struct CodeCollectorConfig {
    /// File extensions to include (without dot)
    pub extensions: Vec<String>,

    /// Directory patterns to search (e.g., "examples", "src")
    pub search_dirs: Vec<String>,

    /// Patterns to exclude (regex)
    pub exclude_patterns: Vec<String>,

    /// Maximum file size in bytes
    pub max_file_size: usize,

    /// Maximum total bytes to collect
    pub max_total_bytes: usize,
}

impl Default for CodeCollectorConfig {
    fn default() -> Self {
        Self {
            extensions: vec![
                "c".to_string(),
                "h".to_string(),
                "cpp".to_string(),
                "hpp".to_string(),
                "rs".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "go".to_string(),
                "java".to_string(),
            ],
            search_dirs: vec![
                "examples".to_string(),
                "samples".to_string(),
                "demo".to_string(),
                "src".to_string(),
                "lib".to_string(),
            ],
            exclude_patterns: vec![
                r"\.git".to_string(),
                r"build".to_string(),
                r"target".to_string(),
                r"node_modules".to_string(),
                r"__pycache__".to_string(),
                r"\.cache".to_string(),
                r"dist".to_string(),
            ],
            max_file_size: 100_000,     // 100KB per file
            max_total_bytes: 2_000_000, // 2MB total
        }
    }
}

/// Collector for source code files
pub struct CodeCollector {
    config: CodeCollectorConfig,
    exclude_regexes: Vec<Regex>,
}

impl CodeCollector {
    /// Create a new code collector with default config
    pub fn new() -> Self {
        Self::with_config(CodeCollectorConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: CodeCollectorConfig) -> Self {
        let exclude_regexes = config
            .exclude_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        Self {
            config,
            exclude_regexes,
        }
    }

    /// Collect source files from project path
    pub fn collect(&self, project_path: &Path) -> Vec<SourceFile> {
        let mut files = Vec::new();
        let mut total_bytes = 0usize;

        // Find directories to search
        let search_paths = self.find_search_dirs(project_path);

        for search_path in search_paths {
            if total_bytes >= self.config.max_total_bytes {
                info!("Reached max total bytes limit, stopping collection");
                break;
            }

            for entry in WalkDir::new(&search_path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if total_bytes >= self.config.max_total_bytes {
                    break;
                }

                let path = entry.path();

                // Skip if not a file
                if !path.is_file() {
                    continue;
                }

                // Skip if excluded
                if self.is_excluded(path) {
                    continue;
                }

                // Check extension
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

                if !self.config.extensions.iter().any(|e| e == ext) {
                    continue;
                }

                // Read file
                match self.read_source_file(project_path, path) {
                    Ok(source_file) => {
                        if source_file.size_bytes <= self.config.max_file_size {
                            total_bytes += source_file.size_bytes;
                            debug!(
                                "Collected: {:?} ({} bytes)",
                                source_file.path, source_file.size_bytes
                            );
                            files.push(source_file);
                        } else {
                            debug!("Skipping large file: {:?}", path);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read {:?}: {}", path, e);
                    }
                }
            }
        }

        info!(
            "Collected {} code files ({} bytes)",
            files.len(),
            total_bytes
        );
        files
    }

    /// Find directories to search within project
    fn find_search_dirs(&self, project_path: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // Check each configured search dir
        for dir_name in &self.config.search_dirs {
            let dir_path = project_path.join(dir_name);
            if dir_path.exists() && dir_path.is_dir() {
                dirs.push(dir_path);
            }
        }

        // Also search direct subdirectories for nested patterns
        if let Ok(entries) = std::fs::read_dir(project_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    // Look for Examples subdirectory (common in SDKs)
                    let examples_dir = path.join("Examples");
                    if examples_dir.exists() {
                        dirs.push(examples_dir);
                    }

                    // Look for examples subdirectory
                    let examples_lower = path.join("examples");
                    if examples_lower.exists() && !dirs.contains(&examples_lower) {
                        dirs.push(examples_lower);
                    }
                }
            }
        }

        // If no specific dirs found, search the whole project (limited)
        if dirs.is_empty() {
            dirs.push(project_path.to_path_buf());
        }

        dirs
    }

    /// Check if path should be excluded
    fn is_excluded(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.exclude_regexes.iter().any(|r| r.is_match(&path_str))
    }

    /// Read a source file and create SourceFile struct
    fn read_source_file(&self, project_root: &Path, path: &Path) -> anyhow::Result<SourceFile> {
        let content = std::fs::read_to_string(path)?;
        let relative_path = path.strip_prefix(project_root).unwrap_or(path).to_path_buf();

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        Ok(SourceFile {
            path: relative_path,
            size_bytes: content.len(),
            language: Language::from_extension(ext),
            content,
        })
    }
}

impl Default for CodeCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for documentation collection
#[derive(Debug, Clone)]
pub struct DocCollectorConfig {
    /// File patterns to include (glob patterns)
    pub patterns: Vec<String>,

    /// Maximum file size in bytes
    pub max_file_size: usize,

    /// Maximum total bytes to collect
    pub max_total_bytes: usize,
}

impl Default for DocCollectorConfig {
    fn default() -> Self {
        Self {
            patterns: vec![
                "README*".to_string(),
                "*.md".to_string(),
                "docs/**/*.md".to_string(),
                "doc/**/*.md".to_string(),
                "documentation/**/*.md".to_string(),
            ],
            max_file_size: 200_000,     // 200KB per file
            max_total_bytes: 1_000_000, // 1MB total
        }
    }
}

/// Collector for documentation files
pub struct DocCollector {
    config: DocCollectorConfig,
}

impl DocCollector {
    /// Create with default config
    pub fn new() -> Self {
        Self::with_config(DocCollectorConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: DocCollectorConfig) -> Self {
        Self { config }
    }

    /// Collect documentation files from project path
    pub fn collect(&self, project_path: &Path) -> Vec<DocFile> {
        let mut files = Vec::new();
        let mut total_bytes = 0usize;

        // Collect using glob patterns
        for pattern in &self.config.patterns {
            if total_bytes >= self.config.max_total_bytes {
                break;
            }

            let full_pattern = project_path.join(pattern);
            let pattern_str = full_pattern.to_string_lossy();

            match glob::glob(&pattern_str) {
                Ok(paths) => {
                    for entry in paths.filter_map(|e| e.ok()) {
                        if total_bytes >= self.config.max_total_bytes {
                            break;
                        }

                        if !entry.is_file() {
                            continue;
                        }

                        match self.read_doc_file(project_path, &entry) {
                            Ok(doc_file) => {
                                if doc_file.size_bytes <= self.config.max_file_size {
                                    // Avoid duplicates
                                    if !files.iter().any(|f: &DocFile| f.path == doc_file.path) {
                                        total_bytes += doc_file.size_bytes;
                                        debug!("Collected doc: {:?}", doc_file.path);
                                        files.push(doc_file);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to read doc {:?}: {}", entry, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Invalid glob pattern '{}': {}", pattern, e);
                }
            }
        }

        info!(
            "Collected {} doc files ({} bytes)",
            files.len(),
            total_bytes
        );
        files
    }

    /// Read a documentation file
    fn read_doc_file(&self, project_root: &Path, path: &Path) -> anyhow::Result<DocFile> {
        let content = std::fs::read_to_string(path)?;
        let relative_path = path.strip_prefix(project_root).unwrap_or(path).to_path_buf();

        Ok(DocFile {
            path: relative_path.clone(),
            size_bytes: content.len(),
            doc_type: DocType::from_path(&relative_path),
            content,
        })
    }
}

impl Default for DocCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Collector for API definitions from source files
///
/// This is a basic implementation that extracts C-style function signatures
/// and macros. Override or extend for other languages.
pub struct ApiCollector {
    /// Regex patterns for extracting API definitions
    function_pattern: Regex,
    #[allow(dead_code)]
    struct_pattern: Regex,
    #[allow(dead_code)]
    typedef_pattern: Regex,
    macro_pattern: Regex,
}

impl ApiCollector {
    /// Create with default patterns (C/C++ focused)
    pub fn new() -> Self {
        Self {
            // Match function declarations like: void func_name(args) { or ;
            function_pattern: Regex::new(r"(?m)^[\t ]*([\w\*]+\s+)+(\w+)\s*\([^)]*\)\s*[;{]")
                .unwrap(),

            // Match struct definitions
            struct_pattern: Regex::new(r"(?m)^\s*typedef\s+struct\s*\w*\s*\{[^}]*\}\s*(\w+)\s*;")
                .unwrap(),

            // Match typedef
            typedef_pattern: Regex::new(r"(?m)^\s*typedef\s+(.+?)\s+(\w+)\s*;").unwrap(),

            // Match macro definitions
            macro_pattern: Regex::new(r"(?m)^#define\s+(\w+)(?:\([^)]*\))?\s+").unwrap(),
        }
    }

    /// Extract API definitions from collected code files
    pub fn collect(&self, code_files: &[SourceFile]) -> Vec<ApiDef> {
        let mut apis = Vec::new();

        for file in code_files {
            // Only process header files by default
            if !file.path.to_string_lossy().ends_with(".h") {
                continue;
            }

            // Extract functions
            for cap in self.function_pattern.captures_iter(&file.content) {
                if let Some(name) = cap.get(2) {
                    let name_str = name.as_str();
                    // Skip common non-API patterns
                    if name_str.starts_with('_') || name_str == "main" {
                        continue;
                    }

                    apis.push(ApiDef {
                        name: name_str.to_string(),
                        kind: ApiKind::Function,
                        signature: cap
                            .get(0)
                            .map(|m| m.as_str().trim().to_string())
                            .unwrap_or_default(),
                        doc_comment: self
                            .find_preceding_comment(&file.content, cap.get(0).unwrap().start()),
                        source_file: file.path.clone(),
                        line_number: self.count_lines(&file.content, cap.get(0).unwrap().start()),
                    });
                }
            }

            // Extract macros (limit to important ones)
            for cap in self.macro_pattern.captures_iter(&file.content) {
                if let Some(name) = cap.get(1) {
                    let name_str = name.as_str();
                    // Only include macro patterns that look like APIs
                    // (start with uppercase or have prefix like HAL_, API_, etc.)
                    if name_str.starts_with("HAL_")
                        || name_str.starts_with("__HAL_")
                        || name_str.starts_with("API_")
                    {
                        apis.push(ApiDef {
                            name: name_str.to_string(),
                            kind: ApiKind::Macro,
                            signature: cap
                                .get(0)
                                .map(|m| m.as_str().trim().to_string())
                                .unwrap_or_default(),
                            doc_comment: None,
                            source_file: file.path.clone(),
                            line_number: self
                                .count_lines(&file.content, cap.get(0).unwrap().start()),
                        });
                    }
                }
            }
        }

        info!("Extracted {} API definitions", apis.len());
        apis
    }

    /// Find comment preceding a position
    fn find_preceding_comment(&self, content: &str, pos: usize) -> Option<String> {
        let prefix = &content[..pos];
        let lines: Vec<&str> = prefix.lines().collect();

        let mut comment_lines = Vec::new();
        for line in lines.iter().rev().take(10) {
            let trimmed = line.trim();
            if trimmed.starts_with("/**")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("*/")
            {
                comment_lines.push(*line);
            } else if trimmed.starts_with("//") {
                comment_lines.push(*line);
            } else if !trimmed.is_empty() {
                break;
            }
        }

        if comment_lines.is_empty() {
            None
        } else {
            comment_lines.reverse();
            Some(comment_lines.join("\n"))
        }
    }

    /// Count lines up to a position
    fn count_lines(&self, content: &str, pos: usize) -> usize {
        content[..pos].lines().count()
    }
}

impl Default for ApiCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Unified collector that combines all collectors
pub struct SdkCollector {
    code_collector: CodeCollector,
    doc_collector: DocCollector,
    api_collector: ApiCollector,
}

impl SdkCollector {
    /// Create with default collectors
    pub fn new() -> Self {
        Self {
            code_collector: CodeCollector::new(),
            doc_collector: DocCollector::new(),
            api_collector: ApiCollector::new(),
        }
    }

    /// Create with custom code collector config
    pub fn with_code_config(config: CodeCollectorConfig) -> Self {
        Self {
            code_collector: CodeCollector::with_config(config),
            doc_collector: DocCollector::new(),
            api_collector: ApiCollector::new(),
        }
    }

    /// Collect all sources from project path
    pub fn collect(&self, project_path: &Path) -> anyhow::Result<CollectedSources> {
        info!("Collecting sources from: {:?}", project_path);

        if !project_path.exists() {
            anyhow::bail!("Project path does not exist: {:?}", project_path);
        }

        let code_files = self.code_collector.collect(project_path);
        let doc_files = self.doc_collector.collect(project_path);
        let api_defs = self.api_collector.collect(&code_files);

        let total_bytes = code_files.iter().map(|f| f.size_bytes).sum::<usize>()
            + doc_files.iter().map(|f| f.size_bytes).sum::<usize>();

        let sources = CollectedSources {
            code_files,
            doc_files,
            api_defs,
            total_bytes,
            collected_at: Some(chrono::Utc::now()),
        };

        info!("Collection complete: {}", sources.summary());
        Ok(sources)
    }
}

impl Default for SdkCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_collector_default_config() {
        let config = CodeCollectorConfig::default();
        assert!(config.extensions.contains(&"c".to_string()));
        assert!(config.extensions.contains(&"h".to_string()));
        assert!(config.extensions.contains(&"rs".to_string()));
        assert!(config.extensions.contains(&"py".to_string()));
    }

    #[test]
    fn test_doc_collector_default_config() {
        let config = DocCollectorConfig::default();
        assert!(config.patterns.iter().any(|p| p.contains("README")));
        assert!(config.patterns.iter().any(|p| p.contains("*.md")));
    }

    #[test]
    fn test_api_collector_function_pattern() {
        let collector = ApiCollector::new();
        let content = "void HAL_GPIO_Init(GPIO_TypeDef *GPIOx, GPIO_InitTypeDef *GPIO_Init);";

        let caps: Vec<_> = collector.function_pattern.captures_iter(content).collect();
        assert!(!caps.is_empty());
    }
}
