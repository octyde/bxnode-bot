//! Web tools for internet access - web search and web fetch.
//!
//! Provides the agent with ability to search the web and fetch URL content.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::RwLock;

use super::tools::Tool;

/// Cache entry with TTL
struct CacheEntry {
    value: serde_json::Value,
    expires_at: Instant,
}

/// Shared cache for web tools
#[derive(Default, Clone)]
pub struct WebCache {
    inner: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl WebCache {
    pub fn new() -> Self {
        Self::default()
    }

    async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let cache = self.inner.read().await;
        cache.get(key).and_then(|entry| {
            if entry.expires_at > Instant::now() {
                Some(entry.value.clone())
            } else {
                None
            }
        })
    }

    async fn set(&self, key: String, value: serde_json::Value, ttl: Duration) {
        let mut cache = self.inner.write().await;
        cache.insert(
            key,
            CacheEntry {
                value,
                expires_at: Instant::now() + ttl,
            },
        );
        // Evict expired entries if cache is large
        if cache.len() > 200 {
            let now = Instant::now();
            cache.retain(|_, v| v.expires_at > now);
        }
    }
}

/// Default cache TTL (15 minutes)
const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

/// Maximum response size for web fetch (1 MB)
const MAX_RESPONSE_BYTES: usize = 1_024 * 1024;

/// Default max chars to return
const DEFAULT_MAX_CHARS: usize = 20_000;

/// HTTP request timeout
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of search results
const MAX_SEARCH_RESULTS: u64 = 10;

/// Web search configuration
#[derive(Debug, Clone, Default)]
pub struct WebSearchConfig {
    pub brave_api_key: Option<String>,
    pub perplexity_api_key: Option<String>,
}

// ============================================================================
// web_search
// ============================================================================

pub struct WebSearchTool {
    config: WebSearchConfig,
    cache: WebCache,
    client: reqwest::Client,
}

impl WebSearchTool {
    pub fn new(config: WebSearchConfig, cache: WebCache) -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("BXNode-Bot/1.0")
            .build()
            .unwrap_or_default();
        Self {
            config,
            cache,
            client,
        }
    }

    async fn search_brave(
        &self,
        query: &str,
        max_results: u64,
    ) -> anyhow::Result<serde_json::Value> {
        let api_key = self
            .config
            .brave_api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Brave Search API key not configured"))?;

        let resp = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .query(&[("q", query), ("count", &max_results.to_string())])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Brave Search API error ({}): {}", status, body);
        }

        let data: serde_json::Value = resp.json().await?;

        let results: Vec<serde_json::Value> = data
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .take(max_results as usize)
                    .map(|r| {
                        json!({
                            "title": r.get("title").and_then(|t| t.as_str()).unwrap_or(""),
                            "url": r.get("url").and_then(|u| u.as_str()).unwrap_or(""),
                            "snippet": r.get("description").and_then(|d| d.as_str()).unwrap_or("")
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(json!({
            "provider": "brave",
            "query": query,
            "count": results.len(),
            "results": results
        }))
    }

    async fn search_perplexity(
        &self,
        query: &str,
        max_results: u64,
    ) -> anyhow::Result<serde_json::Value> {
        let api_key = self
            .config
            .perplexity_api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Perplexity API key not configured"))?;

        let resp = self
            .client
            .post("https://api.perplexity.ai/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&json!({
                "model": "sonar",
                "messages": [{
                    "role": "user",
                    "content": format!(
                        "Search the web for: {}. Return the top {} results with title, URL, and a brief snippet for each.",
                        query, max_results
                    )
                }]
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Perplexity API error ({}): {}", status, body);
        }

        let data: serde_json::Value = resp.json().await?;

        let content = data
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        // Extract citations if available
        let citations: Vec<serde_json::Value> = data
            .get("citations")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .take(max_results as usize)
                    .map(|url| {
                        json!({
                            "title": "",
                            "url": url.as_str().unwrap_or(""),
                            "snippet": ""
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(json!({
            "provider": "perplexity",
            "query": query,
            "answer": content,
            "count": citations.len(),
            "results": citations
        }))
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information. Returns search results with titles, URLs, and snippets. Use this to find current information, documentation, or answers to questions."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5, max: 10)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 10
                },
                "provider": {
                    "type": "string",
                    "description": "Search provider to use: 'brave' or 'perplexity'. Auto-detected if not specified.",
                    "enum": ["brave", "perplexity"]
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(MAX_SEARCH_RESULTS);
        let provider = input.get("provider").and_then(|v| v.as_str());

        // Check cache
        let cache_key = format!(
            "search:{}:{}:{}",
            provider.unwrap_or("auto"),
            query,
            max_results
        );
        if let Some(cached) = self.cache.get(&cache_key).await {
            return Ok(cached.to_string());
        }

        // Determine provider
        let result = match provider {
            Some("brave") => self.search_brave(query, max_results).await?,
            Some("perplexity") => self.search_perplexity(query, max_results).await?,
            _ => {
                // Auto-detect: prefer brave, fallback to perplexity
                if self.config.brave_api_key.is_some() {
                    self.search_brave(query, max_results).await?
                } else if self.config.perplexity_api_key.is_some() {
                    self.search_perplexity(query, max_results).await?
                } else {
                    anyhow::bail!(
                        "No web search provider configured. Set brave_api_key or perplexity_api_key in tools.web config."
                    );
                }
            }
        };

        // Cache result
        self.cache.set(cache_key, result.clone(), CACHE_TTL).await;

        Ok(result.to_string())
    }
}

// ============================================================================
// web_fetch
// ============================================================================

pub struct WebFetchTool {
    cache: WebCache,
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new(cache: WebCache) -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(
                "BXNode-Bot/1.0 (compatible; +https://github.com/octyde/bxnode-bot)",
            )
            .build()
            .unwrap_or_default();
        Self { cache, client }
    }
}

/// Check if a URL targets a private/internal IP (SSRF protection)
fn is_private_url(url_str: &str) -> bool {
    let parsed = match url::Url::parse(url_str) {
        Ok(u) => u,
        Err(_) => return true, // Block unparseable URLs
    };

    // Block non-HTTP(S) schemes
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return true,
    }

    // Check host
    if let Some(host) = parsed.host_str() {
        // Block localhost variants
        if host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "0.0.0.0"
            || host.ends_with(".local")
            || host.ends_with(".internal")
        {
            return true;
        }

        // Try parsing as IP address (strip brackets for IPv6)
        let ip_str = host.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = ip_str.parse::<IpAddr>() {
            return match ip {
                IpAddr::V4(ipv4) => {
                    ipv4.is_private()
                        || ipv4.is_loopback()
                        || ipv4.is_link_local()
                        || ipv4.is_unspecified()
                        // CGNAT range 100.64.0.0/10
                        || (ipv4.octets()[0] == 100
                            && ipv4.octets()[1] >= 64
                            && ipv4.octets()[1] <= 127)
                }
                IpAddr::V6(ipv6) => ipv6.is_loopback() || ipv6.is_unspecified(),
            };
        }
    } else {
        return true; // No host = block
    }

    false
}

/// Extract readable text content from HTML
fn extract_readable_content(html: &str, mode: &str) -> (Option<String>, String) {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);

    // Extract title
    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_string());

    // Try main content selectors first, fall back to body
    let content_selectors = [
        "main",
        "article",
        "[role=\"main\"]",
        ".content",
        "#content",
        "body",
    ];

    let mut body_element = None;
    for selector_str in &content_selectors {
        if let Ok(sel) = Selector::parse(selector_str) {
            if let Some(el) = document.select(&sel).next() {
                body_element = Some(el);
                break;
            }
        }
    }

    let text = if let Some(body) = body_element {
        let mut text = String::new();
        extract_text_from_element(&body, &mut text, mode);
        text
    } else {
        // Fallback: just get all text
        document
            .root_element()
            .text()
            .collect::<Vec<_>>()
            .join(" ")
    };

    // Clean up whitespace
    let cleaned = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    (title, cleaned)
}

/// Recursively extract text from an HTML element
fn extract_text_from_element(
    element: &scraper::ElementRef,
    output: &mut String,
    mode: &str,
) {
    use scraper::Node;

    for child in element.children() {
        match child.value() {
            Node::Text(text) => {
                let t = text.trim();
                if !t.is_empty() {
                    output.push_str(t);
                    output.push(' ');
                }
            }
            Node::Element(el) => {
                let tag = el.name();
                // Skip script, style, nav, footer, aside
                if matches!(
                    tag,
                    "script" | "style" | "noscript" | "nav" | "footer" | "aside" | "iframe"
                ) {
                    continue;
                }

                if let Some(child_ref) = scraper::ElementRef::wrap(child) {
                    // Add formatting for block elements
                    if matches!(
                        tag,
                        "p" | "div"
                            | "br"
                            | "h1"
                            | "h2"
                            | "h3"
                            | "h4"
                            | "h5"
                            | "h6"
                            | "li"
                            | "tr"
                            | "blockquote"
                    ) {
                        output.push('\n');
                    }

                    if mode == "markdown" {
                        match tag {
                            "h1" => output.push_str("# "),
                            "h2" => output.push_str("## "),
                            "h3" => output.push_str("### "),
                            "h4" => output.push_str("#### "),
                            "li" => output.push_str("- "),
                            "a" => {
                                let href = el.attr("href").unwrap_or("");
                                let link_text: String =
                                    child_ref.text().collect::<Vec<_>>().join("");
                                if !link_text.trim().is_empty() && !href.is_empty() {
                                    output.push_str(&format!(
                                        "[{}]({})",
                                        link_text.trim(),
                                        href
                                    ));
                                    output.push(' ');
                                    continue;
                                }
                            }
                            "code" => {
                                let code_text: String =
                                    child_ref.text().collect::<Vec<_>>().join("");
                                output.push_str(&format!("`{}`", code_text.trim()));
                                output.push(' ');
                                continue;
                            }
                            "pre" => {
                                let code_text: String =
                                    child_ref.text().collect::<Vec<_>>().join("");
                                output.push_str(&format!(
                                    "\n```\n{}\n```\n",
                                    code_text.trim()
                                ));
                                continue;
                            }
                            "strong" | "b" => {
                                let bold_text: String =
                                    child_ref.text().collect::<Vec<_>>().join("");
                                output.push_str(&format!("**{}**", bold_text.trim()));
                                output.push(' ');
                                continue;
                            }
                            "em" | "i" => {
                                let em_text: String =
                                    child_ref.text().collect::<Vec<_>>().join("");
                                output.push_str(&format!("*{}*", em_text.trim()));
                                output.push(' ');
                                continue;
                            }
                            _ => {}
                        }
                    }

                    extract_text_from_element(&child_ref, output, mode);

                    if matches!(
                        tag,
                        "p" | "div"
                            | "h1"
                            | "h2"
                            | "h3"
                            | "h4"
                            | "h5"
                            | "h6"
                            | "blockquote"
                    ) {
                        output.push('\n');
                    }
                }
            }
            _ => {}
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch and extract readable content from a URL. Converts HTML pages to clean text or markdown. Use this to read web pages, documentation, API responses, etc."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch (must be http or https)"
                },
                "extract_mode": {
                    "type": "string",
                    "description": "Content extraction mode: 'markdown' (default) or 'text'",
                    "enum": ["markdown", "text"],
                    "default": "markdown"
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Maximum characters to return (default: 20000)",
                    "default": 20000,
                    "minimum": 100
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;
        let extract_mode = input
            .get("extract_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("markdown");
        let max_chars = input
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_CHARS as u64) as usize;

        // SSRF protection
        if is_private_url(url) {
            anyhow::bail!("Cannot fetch private/internal URLs for security reasons");
        }

        // Check cache
        let cache_key = format!("fetch:{}:{}", extract_mode, url);
        if let Some(cached) = self.cache.get(&cache_key).await {
            return Ok(cached.to_string());
        }

        // Fetch the URL
        let resp = self.client.get(url).send().await?;

        if !resp.status().is_success() {
            anyhow::bail!("HTTP error {}: {}", resp.status(), url);
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Read response body with size limit
        let bytes = resp.bytes().await?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            anyhow::bail!(
                "Response too large ({} bytes, max {} bytes)",
                bytes.len(),
                MAX_RESPONSE_BYTES
            );
        }

        let body = String::from_utf8_lossy(&bytes);

        // Extract content based on type
        let (title, content) = if content_type.contains("text/html")
            || content_type.contains("application/xhtml")
        {
            extract_readable_content(&body, extract_mode)
        } else if content_type.contains("application/json") {
            (None, body.to_string())
        } else {
            // Plain text or other
            (None, body.to_string())
        };

        // Truncate if needed
        let truncated = if content.len() > max_chars {
            format!(
                "{}... (truncated, {} chars total)",
                &content[..max_chars],
                content.len()
            )
        } else {
            content.clone()
        };

        let result = json!({
            "url": url,
            "title": title.unwrap_or_default(),
            "content_type": content_type,
            "content_length": content.len(),
            "content": truncated
        });

        // Cache result
        self.cache.set(cache_key, result.clone(), CACHE_TTL).await;

        Ok(result.to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_private_url() {
        assert!(is_private_url("http://localhost:8080"));
        assert!(is_private_url("http://127.0.0.1/api"));
        assert!(is_private_url("http://192.168.1.1/admin"));
        assert!(is_private_url("http://10.0.0.1/secret"));
        assert!(is_private_url("http://172.16.0.1/data"));
        assert!(is_private_url("http://0.0.0.0:3000"));
        assert!(is_private_url("http://[::1]/api"));
        assert!(is_private_url("ftp://example.com/file"));
        assert!(is_private_url("file:///etc/passwd"));

        assert!(!is_private_url("https://example.com"));
        assert!(!is_private_url("https://api.brave.com/search"));
        assert!(!is_private_url("http://8.8.8.8/dns"));
    }

    #[test]
    fn test_extract_readable_content_text() {
        let html = r#"
        <html>
        <head><title>Test Page</title></head>
        <body>
            <h1>Hello World</h1>
            <p>This is a test paragraph.</p>
            <script>alert('evil');</script>
            <style>.hidden { display: none; }</style>
            <p>Another paragraph.</p>
        </body>
        </html>
        "#;

        let (title, content) = extract_readable_content(html, "text");
        assert_eq!(title.unwrap(), "Test Page");
        assert!(content.contains("Hello World"));
        assert!(content.contains("This is a test paragraph"));
        assert!(content.contains("Another paragraph"));
        assert!(!content.contains("alert"));
        assert!(!content.contains("display: none"));
    }

    #[test]
    fn test_extract_readable_content_markdown() {
        let html = r#"
        <html>
        <body>
            <h1>Title</h1>
            <h2>Subtitle</h2>
            <p>Text with <strong>bold</strong> and <em>italic</em>.</p>
            <ul><li>Item 1</li><li>Item 2</li></ul>
            <pre>code block</pre>
        </body>
        </html>
        "#;

        let (_, content) = extract_readable_content(html, "markdown");
        assert!(content.contains("# Title"));
        assert!(content.contains("## Subtitle"));
        assert!(content.contains("**bold**"));
        assert!(content.contains("*italic*"));
        assert!(content.contains("- Item 1"));
        assert!(content.contains("```"));
    }

    #[tokio::test]
    async fn test_web_cache() {
        let cache = WebCache::new();

        // Set and get
        cache
            .set(
                "key1".to_string(),
                json!({"test": true}),
                Duration::from_secs(60),
            )
            .await;
        let val = cache.get("key1").await;
        assert!(val.is_some());
        assert_eq!(val.unwrap()["test"], true);

        // Miss
        let val = cache.get("nonexistent").await;
        assert!(val.is_none());

        // Expired
        cache
            .set(
                "key2".to_string(),
                json!({"expired": true}),
                Duration::from_millis(1),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        let val = cache.get("key2").await;
        assert!(val.is_none());
    }
}
