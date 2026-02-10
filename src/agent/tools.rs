//! Tool system for agents

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;

use crate::memory::{MemoryRecord, MemoryScope, MemoryStore};

/// Tool definition for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,

    /// Tool description
    pub description: String,

    /// Input schema (JSON Schema)
    pub input_schema: serde_json::Value,
}

/// Tool call from LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID (for correlation)
    pub id: String,

    /// Tool name
    pub name: String,

    /// Tool input arguments
    pub input: serde_json::Value,
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Tool call ID (for correlation)
    pub tool_call_id: String,

    /// Whether the tool execution was successful
    pub success: bool,

    /// Result content
    pub content: String,

    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            success: true,
            content: content.into(),
            error: None,
        }
    }

    pub fn error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            success: false,
            content: String::new(),
            error: Some(error.into()),
        }
    }
}

/// Tool trait - implement this for each tool
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (must be unique)
    fn name(&self) -> &str;

    /// Tool description for the LLM
    fn description(&self) -> &str;

    /// Input schema (JSON Schema format)
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the tool with given input
    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String>;

    /// Get the tool definition for LLM consumption
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.input_schema(),
        }
    }
}

/// Tool registry for managing available tools
#[derive(Default)]
pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with built-in tools
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(EchoTool));
        registry.register(Box::new(CurrentTimeTool));
        registry
    }

    /// Create a registry with built-in tools and memory tools
    pub fn with_memory(memory: Arc<RwLock<MemoryStore>>, scope: MemoryScope) -> Self {
        let mut registry = Self::with_builtins();
        registry.register(Box::new(MemoryStoreTool::new(memory.clone(), scope.clone())));
        registry.register(Box::new(MemoryRecallTool::new(memory.clone(), scope.clone())));
        registry.register(Box::new(MemoryForgetTool::new(memory, scope)));
        registry
    }

    /// Register a tool
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Unregister a tool
    pub fn unregister(&mut self, name: &str) -> Option<Box<dyn Tool>> {
        self.tools.remove(name)
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Check if a tool exists
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// List all registered tools
    pub fn list(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|t| t.definition())
            .collect()
    }

    /// Get tool names
    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Execute a tool call
    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        match self.get(&call.name) {
            Some(tool) => match tool.execute(call.input.clone()).await {
                Ok(content) => ToolResult::success(&call.id, content),
                Err(e) => ToolResult::error(&call.id, e.to_string()),
            },
            None => ToolResult::error(&call.id, format!("Tool not found: {}", call.name)),
        }
    }

    /// Execute multiple tool calls
    pub async fn execute_all(&self, calls: &[ToolCall]) -> Vec<ToolResult> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            results.push(self.execute(call).await);
        }
        results
    }
}

// ============================================================================
// Built-in Tools
// ============================================================================

/// Echo tool - returns the input back (useful for testing)
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes back the provided message. Useful for testing tool calls."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to echo back"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'message' parameter"))?;
        Ok(message.to_string())
    }
}

/// Current time tool - returns the current date and time
pub struct CurrentTimeTool;

#[async_trait]
impl Tool for CurrentTimeTool {
    fn name(&self) -> &str {
        "current_time"
    }

    fn description(&self) -> &str {
        "Returns the current date and time in the specified timezone."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "timezone": {
                    "type": "string",
                    "description": "Timezone in IANA format (e.g., 'America/New_York'). Defaults to UTC.",
                    "default": "UTC"
                },
                "format": {
                    "type": "string",
                    "description": "Output format: 'iso8601', 'rfc2822', or 'human'. Defaults to 'iso8601'.",
                    "enum": ["iso8601", "rfc2822", "human"],
                    "default": "iso8601"
                }
            }
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let format = input
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("iso8601");

        let now = chrono::Utc::now();

        let result = match format {
            "rfc2822" => now.to_rfc2822(),
            "human" => now.format("%B %d, %Y at %H:%M:%S UTC").to_string(),
            _ => now.to_rfc3339(),
        };

        Ok(result)
    }
}

/// Calculator tool - performs basic arithmetic
pub struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Performs basic arithmetic operations: add, subtract, multiply, divide."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "description": "The arithmetic operation to perform",
                    "enum": ["add", "subtract", "multiply", "divide"]
                },
                "a": {
                    "type": "number",
                    "description": "First operand"
                },
                "b": {
                    "type": "number",
                    "description": "Second operand"
                }
            },
            "required": ["operation", "a", "b"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let operation = input
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'operation' parameter"))?;

        let a = input
            .get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'a' parameter"))?;

        let b = input
            .get("b")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'b' parameter"))?;

        let result = match operation {
            "add" => a + b,
            "subtract" => a - b,
            "multiply" => a * b,
            "divide" => {
                if b == 0.0 {
                    anyhow::bail!("Division by zero");
                }
                a / b
            }
            _ => anyhow::bail!("Unknown operation: {}", operation),
        };

        Ok(result.to_string())
    }
}

// ============================================================================
// Memory Tools
// ============================================================================

/// Memory store tool - stores information in long-term memory
pub struct MemoryStoreTool {
    store: Arc<RwLock<MemoryStore>>,
    scope: MemoryScope,
}

impl MemoryStoreTool {
    pub fn new(store: Arc<RwLock<MemoryStore>>, scope: MemoryScope) -> Self {
        Self { store, scope }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    fn name(&self) -> &str {
        "memory_store"
    }

    fn description(&self) -> &str {
        "Store information in long-term memory for later recall. Use this to remember important facts, user preferences, or context that should persist across conversations."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to remember"
                },
                "summary": {
                    "type": "string",
                    "description": "A short summary of the content (optional, for search results)"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags to help categorize and find this memory"
                },
                "importance": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 10,
                    "default": 5,
                    "description": "Importance level (0-10). Higher values are prioritized in search results."
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;

        let summary = input
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let tags: Vec<String> = input
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let importance = input
            .get("importance")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u8;

        let record = MemoryRecord::builder()
            .scope(self.scope.clone())
            .content(content)
            .summary_opt(summary)
            .tags(tags)
            .importance(importance.min(10))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build memory record: {}", e))?;

        let id = {
            let mut store = self.store.write().await;
            store.store(record)?
        };

        Ok(json!({
            "stored": true,
            "id": id,
            "message": "Memory stored successfully"
        })
        .to_string())
    }
}

/// Memory recall tool - searches long-term memory
pub struct MemoryRecallTool {
    store: Arc<RwLock<MemoryStore>>,
    scope: MemoryScope,
}

impl MemoryRecallTool {
    pub fn new(store: Arc<RwLock<MemoryStore>>, scope: MemoryScope) -> Self {
        Self { store, scope }
    }
}

#[async_trait]
impl Tool for MemoryRecallTool {
    fn name(&self) -> &str {
        "memory_recall"
    }

    fn description(&self) -> &str {
        "Search long-term memory for relevant information. Returns memories matching the query, ranked by relevance."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to find relevant memories"
                },
                "limit": {
                    "type": "integer",
                    "default": 5,
                    "maximum": 20,
                    "description": "Maximum number of results to return"
                },
                "min_importance": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 10,
                    "description": "Only return memories with at least this importance level"
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

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let min_importance = input
            .get("min_importance")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8);

        let results = {
            let store = self.store.read().await;
            match min_importance {
                Some(min) => store.search_with_importance(query, &self.scope, limit, min),
                None => store.search(query, &self.scope, limit),
            }
        };

        if results.is_empty() {
            return Ok(json!({
                "found": false,
                "count": 0,
                "memories": [],
                "message": "No memories found matching the query"
            })
            .to_string());
        }

        let memories: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "score": format!("{:.2}", r.score),
                    "summary": r.summary,
                    "content_preview": r.content_preview,
                    "tags": r.tags,
                    "importance": r.importance,
                    "created_at": r.created_at
                })
            })
            .collect();

        Ok(json!({
            "found": true,
            "count": memories.len(),
            "memories": memories
        })
        .to_string())
    }
}

/// Memory forget tool - removes a memory by ID
pub struct MemoryForgetTool {
    store: Arc<RwLock<MemoryStore>>,
    scope: MemoryScope,
}

impl MemoryForgetTool {
    pub fn new(store: Arc<RwLock<MemoryStore>>, scope: MemoryScope) -> Self {
        Self { store, scope }
    }
}

#[async_trait]
impl Tool for MemoryForgetTool {
    fn name(&self) -> &str {
        "memory_forget"
    }

    fn description(&self) -> &str {
        "Remove a memory by its ID. The memory will be soft-deleted (kept for audit but no longer searchable)."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The ID of the memory to forget"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let id = input
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;

        // Verify the memory belongs to this scope before deleting
        let can_delete = {
            let store = self.store.read().await;
            if let Some(record) = store.get(id) {
                record.scope.matches(&self.scope)
            } else {
                false
            }
        };

        if !can_delete {
            return Ok(json!({
                "deleted": false,
                "id": id,
                "message": "Memory not found or access denied"
            })
            .to_string());
        }

        let deleted = {
            let mut store = self.store.write().await;
            store.delete(id)?
        };

        Ok(json!({
            "deleted": deleted,
            "id": id,
            "message": if deleted { "Memory forgotten" } else { "Memory was already deleted" }
        })
        .to_string())
    }
}

// ============================================================================
// Provider-specific tool formats
// ============================================================================

/// Convert tool definitions to Anthropic format
pub fn to_anthropic_tools(tools: &[ToolDefinition]) -> serde_json::Value {
    json!(tools.iter().map(|t| {
        json!({
            "name": t.name,
            "description": t.description,
            "input_schema": t.input_schema
        })
    }).collect::<Vec<_>>())
}

/// Convert tool definitions to OpenAI format
pub fn to_openai_tools(tools: &[ToolDefinition]) -> serde_json::Value {
    json!(tools.iter().map(|t| {
        json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.input_schema
            }
        })
    }).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("call-123", "Hello, world!");
        assert!(result.success);
        assert_eq!(result.tool_call_id, "call-123");
        assert_eq!(result.content, "Hello, world!");
        assert!(result.error.is_none());
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("call-456", "Something went wrong");
        assert!(!result.success);
        assert_eq!(result.tool_call_id, "call-456");
        assert!(result.content.is_empty());
        assert_eq!(result.error, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_tool_registry_with_builtins() {
        let registry = ToolRegistry::with_builtins();
        assert!(registry.contains("echo"));
        assert!(registry.contains("current_time"));
    }

    #[tokio::test]
    async fn test_echo_tool() {
        let tool = EchoTool;
        let result = tool.execute(json!({"message": "test"})).await.unwrap();
        assert_eq!(result, "test");
    }

    #[tokio::test]
    async fn test_current_time_tool() {
        let tool = CurrentTimeTool;
        let result = tool.execute(json!({})).await.unwrap();
        // Should be a valid ISO8601 timestamp
        assert!(result.contains("T"));
    }

    #[tokio::test]
    async fn test_calculator_tool() {
        let tool = CalculatorTool;

        let result = tool
            .execute(json!({"operation": "add", "a": 2, "b": 3}))
            .await
            .unwrap();
        assert_eq!(result, "5");

        let result = tool
            .execute(json!({"operation": "multiply", "a": 4, "b": 5}))
            .await
            .unwrap();
        assert_eq!(result, "20");

        let result = tool
            .execute(json!({"operation": "divide", "a": 10, "b": 0}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tool_registry_execute() {
        let registry = ToolRegistry::with_builtins();

        let call = ToolCall {
            id: "test-1".to_string(),
            name: "echo".to_string(),
            input: json!({"message": "hello"}),
        };

        let result = registry.execute(&call).await;
        assert!(result.success);
        assert_eq!(result.content, "hello");
    }

    #[tokio::test]
    async fn test_tool_registry_execute_unknown() {
        let registry = ToolRegistry::new();

        let call = ToolCall {
            id: "test-2".to_string(),
            name: "unknown".to_string(),
            input: json!({}),
        };

        let result = registry.execute(&call).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[test]
    fn test_to_anthropic_tools() {
        let tools = vec![ToolDefinition {
            name: "test".to_string(),
            description: "A test tool".to_string(),
            input_schema: json!({"type": "object"}),
        }];

        let result = to_anthropic_tools(&tools);
        assert!(result.is_array());
        assert_eq!(result[0]["name"], "test");
    }

    #[test]
    fn test_to_openai_tools() {
        let tools = vec![ToolDefinition {
            name: "test".to_string(),
            description: "A test tool".to_string(),
            input_schema: json!({"type": "object"}),
        }];

        let result = to_openai_tools(&tools);
        assert!(result.is_array());
        assert_eq!(result[0]["type"], "function");
        assert_eq!(result[0]["function"]["name"], "test");
    }
}
