//! Unit tests for the tools module

use super::tools::*;
use async_trait::async_trait;
use serde_json::json;

/// Mock tool for testing
struct MockTool {
    name: String,
    description: String,
    should_succeed: bool,
}

impl MockTool {
    fn new(name: &str, description: &str, should_succeed: bool) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            should_succeed,
        }
    }
}

#[async_trait]
impl Tool for MockTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            },
            "required": ["input"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        if self.should_succeed {
            Ok(format!("Executed {} with input: {:?}", self.name, input))
        } else {
            anyhow::bail!("Mock execution failed")
        }
    }
}

#[test]
fn test_tool_definition_creation() {
    let def = ToolDefinition {
        name: "weather".to_string(),
        description: "Get weather information".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "location": { "type": "string" }
            }
        }),
    };

    assert_eq!(def.name, "weather");
    assert_eq!(def.description, "Get weather information");
    assert!(def.input_schema.is_object());
}

#[test]
fn test_tool_definition_serialization() {
    let def = ToolDefinition {
        name: "search".to_string(),
        description: "Search the web".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        }),
    };

    let json = serde_json::to_value(&def).unwrap();

    assert_eq!(json["name"], "search");
    assert_eq!(json["description"], "Search the web");
    assert!(json["input_schema"]["properties"]["query"].is_object());
}

#[test]
fn test_tool_result_success() {
    let result = ToolResult::success("call-123", "Operation completed");

    assert!(result.success);
    assert_eq!(result.tool_call_id, "call-123");
    assert_eq!(result.content, "Operation completed");
    assert!(result.error.is_none());
}

#[test]
fn test_tool_result_failure() {
    let result = ToolResult::error("call-456", "Permission denied");

    assert!(!result.success);
    assert_eq!(result.tool_call_id, "call-456");
    assert!(result.content.is_empty());
    assert_eq!(result.error, Some("Permission denied".to_string()));
}

#[test]
fn test_tool_result_serialization() {
    let result = ToolResult::success("call-789", "Result data");

    let json = serde_json::to_value(&result).unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["content"], "Result data");
    assert_eq!(json["tool_call_id"], "call-789");
}

#[test]
fn test_tool_registry_new() {
    let registry = ToolRegistry::new();

    assert!(registry.list().is_empty());
}

#[test]
fn test_tool_registry_register_and_get() {
    let mut registry = ToolRegistry::new();
    let tool = Box::new(MockTool::new("test_tool", "A test tool", true));

    registry.register(tool);

    let retrieved = registry.get("test_tool");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name(), "test_tool");
}

#[test]
fn test_tool_registry_get_nonexistent() {
    let registry = ToolRegistry::new();

    let retrieved = registry.get("nonexistent");
    assert!(retrieved.is_none());
}

#[test]
fn test_tool_registry_list() {
    let mut registry = ToolRegistry::new();

    registry.register(Box::new(MockTool::new("tool1", "First tool", true)));
    registry.register(Box::new(MockTool::new("tool2", "Second tool", true)));
    registry.register(Box::new(MockTool::new("tool3", "Third tool", false)));

    let definitions = registry.list();

    assert_eq!(definitions.len(), 3);

    // Check that all tools are in the list (order may vary due to HashMap)
    let names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"tool1"));
    assert!(names.contains(&"tool2"));
    assert!(names.contains(&"tool3"));
}

#[test]
fn test_tool_registry_register_overwrites() {
    let mut registry = ToolRegistry::new();

    registry.register(Box::new(MockTool::new("duplicate", "First version", true)));
    registry.register(Box::new(MockTool::new("duplicate", "Second version", false)));

    let definitions = registry.list();
    assert_eq!(definitions.len(), 1);

    let tool = registry.get("duplicate").unwrap();
    assert_eq!(tool.description(), "Second version");
}

#[tokio::test]
async fn test_mock_tool_execute_success() {
    let tool = MockTool::new("success_tool", "Always succeeds", true);

    let result = tool.execute(json!({"input": "test"})).await.unwrap();

    assert!(result.contains("success_tool"));
}

#[tokio::test]
async fn test_mock_tool_execute_failure() {
    let tool = MockTool::new("fail_tool", "Always fails", false);

    let result = tool.execute(json!({"input": "test"})).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_tool_registry_execute() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(MockTool::new("exec_tool", "Executable tool", true)));

    let call = ToolCall {
        id: "call-1".to_string(),
        name: "exec_tool".to_string(),
        input: json!({"input": "hello"}),
    };

    let result = registry.execute(&call).await;

    assert!(result.success);
    assert_eq!(result.tool_call_id, "call-1");
}

#[tokio::test]
async fn test_tool_registry_execute_unknown() {
    let registry = ToolRegistry::new();

    let call = ToolCall {
        id: "call-2".to_string(),
        name: "unknown".to_string(),
        input: json!({}),
    };

    let result = registry.execute(&call).await;

    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[test]
fn test_tool_trait_implementation() {
    let tool = MockTool::new("trait_test", "Testing trait methods", true);

    assert_eq!(tool.name(), "trait_test");
    assert_eq!(tool.description(), "Testing trait methods");

    let schema = tool.input_schema();
    assert!(schema.is_object());
    assert!(schema["properties"]["input"].is_object());
}

#[test]
fn test_tool_definition_deserialization() {
    let json = json!({
        "name": "calculator",
        "description": "Perform calculations",
        "input_schema": {
            "type": "object",
            "properties": {
                "expression": { "type": "string" }
            }
        }
    });

    let def: ToolDefinition = serde_json::from_value(json).unwrap();

    assert_eq!(def.name, "calculator");
    assert_eq!(def.description, "Perform calculations");
}

#[test]
fn test_tool_result_deserialization() {
    let json = json!({
        "tool_call_id": "call-test",
        "success": true,
        "content": "42",
        "error": null
    });

    let result: ToolResult = serde_json::from_value(json).unwrap();

    assert!(result.success);
    assert_eq!(result.content, "42");
    assert!(result.error.is_none());
}

#[test]
fn test_tool_registry_default() {
    let registry = ToolRegistry::default();

    assert!(registry.list().is_empty());
}

#[test]
fn test_tool_definition_with_complex_schema() {
    let def = ToolDefinition {
        name: "api_call".to_string(),
        description: "Make an API call".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "format": "uri" },
                "method": { "type": "string", "enum": ["GET", "POST", "PUT", "DELETE"] },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                },
                "body": { "type": "object" }
            },
            "required": ["url", "method"]
        }),
    };

    let json = serde_json::to_value(&def).unwrap();
    let restored: ToolDefinition = serde_json::from_value(json).unwrap();

    assert_eq!(restored.name, "api_call");
    assert!(restored.input_schema["properties"]["method"]["enum"].is_array());
}

#[test]
fn test_tool_call_serialization() {
    let call = ToolCall {
        id: "call-abc".to_string(),
        name: "echo".to_string(),
        input: json!({"message": "hello"}),
    };

    let json = serde_json::to_value(&call).unwrap();

    assert_eq!(json["id"], "call-abc");
    assert_eq!(json["name"], "echo");
    assert_eq!(json["input"]["message"], "hello");
}

#[test]
fn test_tool_registry_contains() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(MockTool::new("exists", "Exists", true)));

    assert!(registry.contains("exists"));
    assert!(!registry.contains("not_exists"));
}

#[test]
fn test_tool_registry_names() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(MockTool::new("tool_a", "A", true)));
    registry.register(Box::new(MockTool::new("tool_b", "B", true)));

    let names = registry.names();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"tool_a".to_string()));
    assert!(names.contains(&"tool_b".to_string()));
}

#[test]
fn test_tool_registry_unregister() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(MockTool::new("removable", "To be removed", true)));

    assert!(registry.contains("removable"));

    let removed = registry.unregister("removable");
    assert!(removed.is_some());
    assert!(!registry.contains("removable"));
}

#[tokio::test]
async fn test_tool_registry_execute_all() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(MockTool::new("tool1", "Tool 1", true)));
    registry.register(Box::new(MockTool::new("tool2", "Tool 2", true)));

    let calls = vec![
        ToolCall {
            id: "c1".to_string(),
            name: "tool1".to_string(),
            input: json!({"input": "test1"}),
        },
        ToolCall {
            id: "c2".to_string(),
            name: "tool2".to_string(),
            input: json!({"input": "test2"}),
        },
    ];

    let results = registry.execute_all(&calls).await;

    assert_eq!(results.len(), 2);
    assert!(results[0].success);
    assert!(results[1].success);
}
