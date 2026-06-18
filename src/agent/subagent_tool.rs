//! The `spawn_subagents` tool.
//!
//! Exposes [`SubagentDispatcher`] to the LLM so it can fan out independent
//! workstreams in parallel — e.g. "search the web for X, read file Y, and
//! fetch URL Z" run concurrently, one summary per task.
//!
//! The tool builds a fresh child [`ToolRegistry`] via a caller-supplied
//! factory so subagents can't accidentally recurse into themselves; the
//! factory typically omits `spawn_subagents` from the child registry.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::orchestration::{SubagentDispatcher, SubagentTask};
use super::tool_policy::{ToolMetadata, ToolRisk};
use super::tools::{Tool, ToolCall, ToolRegistry};

/// Factory that builds a fresh [`ToolRegistry`] for each subagent invocation.
pub type RegistryFactory = Arc<dyn Fn() -> ToolRegistry + Send + Sync>;

/// Tool that dispatches parallel subagent tasks.
pub struct SpawnSubagentsTool {
    factory: RegistryFactory,
    max_concurrent: usize,
    max_tasks_per_call: usize,
}

impl SpawnSubagentsTool {
    /// Create a new subagent-spawning tool.
    ///
    /// - `factory` builds the child [`ToolRegistry`] each call. Do NOT include
    ///   `spawn_subagents` itself in the child registry — the tool will refuse
    ///   recursive calls, but omitting it is cleaner.
    /// - `max_concurrent` caps in-flight subagents (clamped to at least 1).
    /// - `max_tasks_per_call` rejects calls asking for more tasks than this,
    ///   to prevent a runaway LLM from queueing 500 workstreams.
    pub fn new(factory: RegistryFactory, max_concurrent: usize, max_tasks_per_call: usize) -> Self {
        Self {
            factory,
            max_concurrent: max_concurrent.max(1),
            max_tasks_per_call: max_tasks_per_call.max(1),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SubagentToolInput {
    tasks: Vec<SubagentToolTask>,
}

#[derive(Debug, Deserialize)]
struct SubagentToolTask {
    description: String,
    calls: Vec<SubagentToolCall>,
}

#[derive(Debug, Deserialize)]
struct SubagentToolCall {
    name: String,
    #[serde(default)]
    input: serde_json::Value,
}

#[async_trait]
impl Tool for SpawnSubagentsTool {
    fn name(&self) -> &str {
        "spawn_subagents"
    }

    fn description(&self) -> &str {
        "Run multiple independent workstreams in parallel. Each task is a \
         sequence of tool calls with its own description. Use this to decompose \
         work that has no dependencies between parts (e.g. research several \
         topics at once). Returns one result block per task in input order."
    }

    fn metadata(&self) -> ToolMetadata {
        // Subagents may invoke mutating tools, so mark as mutating at this layer.
        // Concurrency inside the tool is bounded by max_concurrent.
        ToolMetadata {
            read_only: false,
            concurrency_safe: false,
            risk: ToolRisk::Mutating,
            max_result_chars: 16_000,
            exposed_by_default: false,
        }
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["tasks"],
            "properties": {
                "tasks": {
                    "type": "array",
                    "description": "Independent workstreams to run in parallel.",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "required": ["description", "calls"],
                        "properties": {
                            "description": {
                                "type": "string",
                                "description": "Short, human-readable purpose of this task."
                            },
                            "calls": {
                                "type": "array",
                                "description": "Tool calls to execute in order for this task.",
                                "minItems": 1,
                                "items": {
                                    "type": "object",
                                    "required": ["name"],
                                    "properties": {
                                        "name": {
                                            "type": "string",
                                            "description": "Name of a tool registered in the child registry."
                                        },
                                        "input": {
                                            "type": "object",
                                            "description": "JSON input passed to the tool."
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let parsed: SubagentToolInput = serde_json::from_value(input)
            .map_err(|e| anyhow::anyhow!("Invalid spawn_subagents input: {}", e))?;

        if parsed.tasks.is_empty() {
            anyhow::bail!("spawn_subagents requires at least one task");
        }
        if parsed.tasks.len() > self.max_tasks_per_call {
            anyhow::bail!(
                "spawn_subagents: {} tasks exceeds cap of {}",
                parsed.tasks.len(),
                self.max_tasks_per_call
            );
        }

        // Build the child registry and translate inputs into SubagentTasks.
        let child_registry = (self.factory)();
        let tasks: Vec<SubagentTask> = parsed
            .tasks
            .into_iter()
            .enumerate()
            .map(|(task_idx, t)| SubagentTask {
                description: t.description,
                calls: t
                    .calls
                    .into_iter()
                    .enumerate()
                    .map(|(call_idx, c)| ToolCall {
                        // IDs need only be unique within a task; prefix with task index
                        // so repeated ToolResults are traceable in logs.
                        id: format!("sub-{}-{}", task_idx, call_idx),
                        name: c.name,
                        input: c.input,
                    })
                    .collect(),
            })
            .collect();

        let dispatcher = SubagentDispatcher::new(self.max_concurrent);
        let results = dispatcher.dispatch(&child_registry, tasks).await;

        // Render a compact text summary. The full structured output is
        // available via tracing::debug logs if deeper inspection is needed.
        let mut out = String::new();
        for (i, r) in results.iter().enumerate() {
            out.push_str(&format!("[task {}] {}\n", i, r.description));
            for (j, cr) in r.results.iter().enumerate() {
                let status = if cr.success { "ok" } else { "error" };
                let body = if cr.success {
                    cr.content.as_str()
                } else {
                    cr.error.as_deref().unwrap_or("(no error message)")
                };
                out.push_str(&format!("  call {}: {} — {}\n", j, status, body));
            }
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::CurrentTimeTool;

    fn factory_with_time() -> RegistryFactory {
        Arc::new(|| {
            let mut r = ToolRegistry::new();
            r.register(Box::new(CurrentTimeTool));
            r
        })
    }

    #[tokio::test]
    async fn spawns_two_tasks_and_returns_combined_output() {
        let tool = SpawnSubagentsTool::new(factory_with_time(), 4, 10);
        let input = json!({
            "tasks": [
                {
                    "description": "time one",
                    "calls": [{"name": "current_time", "input": {"format": "iso8601"}}]
                },
                {
                    "description": "time two",
                    "calls": [{"name": "current_time", "input": {"format": "human"}}]
                }
            ]
        });

        let out = tool.execute(input).await.unwrap();
        assert!(out.contains("[task 0] time one"));
        assert!(out.contains("[task 1] time two"));
        assert!(out.contains("call 0: ok"));
    }

    #[tokio::test]
    async fn rejects_empty_tasks() {
        let tool = SpawnSubagentsTool::new(factory_with_time(), 4, 10);
        let err = tool.execute(json!({"tasks": []})).await.unwrap_err();
        assert!(err.to_string().contains("at least one task"));
    }

    #[tokio::test]
    async fn rejects_too_many_tasks() {
        let tool = SpawnSubagentsTool::new(factory_with_time(), 4, 2);
        let input = json!({
            "tasks": [
                {"description": "a", "calls": [{"name": "current_time", "input": {}}]},
                {"description": "b", "calls": [{"name": "current_time", "input": {}}]},
                {"description": "c", "calls": [{"name": "current_time", "input": {}}]},
            ]
        });
        let err = tool.execute(input).await.unwrap_err();
        assert!(err.to_string().contains("exceeds cap"));
    }

    #[tokio::test]
    async fn unknown_tool_in_task_returns_per_call_error() {
        let tool = SpawnSubagentsTool::new(factory_with_time(), 4, 10);
        let input = json!({
            "tasks": [
                {
                    "description": "bad tool",
                    "calls": [{"name": "nonexistent_tool", "input": {}}]
                }
            ]
        });
        // Should not bubble up as an Err — the dispatcher records per-call errors.
        let out = tool.execute(input).await.unwrap();
        assert!(out.contains("[task 0] bad tool"));
        assert!(out.contains("error"));
    }

    #[tokio::test]
    async fn clamps_zero_concurrency_to_one() {
        let tool = SpawnSubagentsTool::new(factory_with_time(), 0, 10);
        assert_eq!(tool.max_concurrent, 1);
    }
}
