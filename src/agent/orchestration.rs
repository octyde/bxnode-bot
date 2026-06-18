//! Tool call orchestration for the agent loop.
//!
//! Two layers:
//! - [`ExecutionPlanner`] — groups single-turn tool calls by read/mutate metadata.
//! - [`SubagentDispatcher`] — runs multiple independent workstreams in parallel,
//!   each with its own isolated tool sequence. Inspired by Hermes-agent and
//!   Claude Code's subagent model: the parent decomposes a task, each subagent
//!   gets a bounded task and returns a single summary.

use std::sync::Arc;

use futures_util::future::join_all;
use tokio::sync::Semaphore;

use super::{ToolCall, ToolRegistry, ToolResult};

/// Execution strategy for a group of tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionBatchKind {
    ParallelReadOnly,
    SequentialMutating,
}

/// A batch of tool calls that share the same execution strategy.
#[derive(Debug, Clone)]
pub struct ToolExecutionBatch {
    pub kind: ExecutionBatchKind,
    pub calls: Vec<ToolCall>,
}

/// Planner that groups tool calls using tool metadata.
pub struct ExecutionPlanner;

impl ExecutionPlanner {
    pub fn plan(calls: &[ToolCall], registry: &ToolRegistry) -> Vec<ToolExecutionBatch> {
        let mut batches = Vec::new();
        let mut parallel_batch = Vec::new();

        for call in calls {
            if is_parallel_read_only(call, registry) {
                parallel_batch.push(call.clone());
                continue;
            }

            if !parallel_batch.is_empty() {
                batches.push(ToolExecutionBatch {
                    kind: ExecutionBatchKind::ParallelReadOnly,
                    calls: std::mem::take(&mut parallel_batch),
                });
            }

            batches.push(ToolExecutionBatch {
                kind: ExecutionBatchKind::SequentialMutating,
                calls: vec![call.clone()],
            });
        }

        if !parallel_batch.is_empty() {
            batches.push(ToolExecutionBatch {
                kind: ExecutionBatchKind::ParallelReadOnly,
                calls: parallel_batch,
            });
        }

        batches
    }
}

/// Execute tool calls using metadata-driven batching.
pub async fn execute_tool_batches(registry: &ToolRegistry, calls: &[ToolCall]) -> Vec<ToolResult> {
    let mut results = Vec::with_capacity(calls.len());

    for batch in ExecutionPlanner::plan(calls, registry) {
        match batch.kind {
            ExecutionBatchKind::ParallelReadOnly => {
                let batch_results =
                    join_all(batch.calls.iter().map(|call| registry.execute(call))).await;
                results.extend(batch_results);
            }
            ExecutionBatchKind::SequentialMutating => {
                for call in &batch.calls {
                    results.push(registry.execute(call).await);
                }
            }
        }
    }

    results
}

fn is_parallel_read_only(call: &ToolCall, registry: &ToolRegistry) -> bool {
    registry.get(&call.name).is_some_and(|tool| {
        let metadata = tool.metadata();
        metadata.read_only && metadata.concurrency_safe
    })
}

// =============================================================================
// Subagent dispatch
// =============================================================================

/// A bounded workstream the parent agent wants to run in isolation.
///
/// Each task is a sequence of tool calls with a short description. The
/// dispatcher runs tasks in parallel with a concurrency cap and returns one
/// [`SubagentResult`] per task. Tasks do not share state — a crash or error in
/// one does not block siblings.
#[derive(Debug, Clone)]
pub struct SubagentTask {
    /// Short human-readable description ("research ticket #123", "fetch cat image").
    pub description: String,
    /// Tool calls to execute, in order. Mutating calls run sequentially within
    /// a task; read-only calls within a task still use [`ExecutionPlanner`].
    pub calls: Vec<ToolCall>,
}

/// Outcome of one subagent task.
#[derive(Debug, Clone)]
pub struct SubagentResult {
    pub description: String,
    /// Per-call results, in the same order as `SubagentTask::calls`.
    pub results: Vec<ToolResult>,
}

/// Runs multiple [`SubagentTask`]s in parallel with a concurrency cap.
///
/// Concurrency is capped by `max_concurrent` to prevent runaway fan-out
/// (e.g. a parent spawning 50 web fetches at once). The cap is enforced via
/// a [`tokio::sync::Semaphore`], so tasks queue rather than error when saturated.
pub struct SubagentDispatcher {
    max_concurrent: usize,
}

impl SubagentDispatcher {
    /// Create a dispatcher with the given concurrency cap.
    ///
    /// `max_concurrent` is clamped to at least 1.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent: max_concurrent.max(1),
        }
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    /// Dispatch tasks in parallel and return results in input order.
    pub async fn dispatch(
        &self,
        registry: &ToolRegistry,
        tasks: Vec<SubagentTask>,
    ) -> Vec<SubagentResult> {
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));

        let futures = tasks.into_iter().map(|task| {
            let sem = Arc::clone(&semaphore);
            async move {
                // Hold a permit for the duration of the task. If the semaphore
                // is closed (shouldn't happen here) we fall back to running
                // without a permit rather than dropping the task silently.
                let _permit = sem.acquire_owned().await.ok();
                let results = execute_tool_batches(registry, &task.calls).await;
                SubagentResult {
                    description: task.description,
                    results,
                }
            }
        });

        join_all(futures).await
    }
}

impl Default for SubagentDispatcher {
    fn default() -> Self {
        Self::new(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Tool, ToolMetadata, ToolRisk};
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::Barrier;

    struct BatchTestTool {
        name: String,
        metadata: ToolMetadata,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        barrier: Option<Arc<Barrier>>,
    }

    impl BatchTestTool {
        fn new(
            name: &str,
            metadata: ToolMetadata,
            active: Arc<AtomicUsize>,
            max_active: Arc<AtomicUsize>,
        ) -> Self {
            Self {
                name: name.to_string(),
                metadata,
                active,
                max_active,
                barrier: None,
            }
        }

        fn with_barrier(mut self, barrier: Arc<Barrier>) -> Self {
            self.barrier = Some(barrier);
            self
        }
    }

    #[async_trait]
    impl Tool for BatchTestTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "test tool"
        }

        fn input_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn metadata(&self) -> ToolMetadata {
            self.metadata
        }

        async fn execute(&self, _input: serde_json::Value) -> anyhow::Result<String> {
            let current = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(current, Ordering::SeqCst);

            if let Some(barrier) = &self.barrier {
                barrier.wait().await;
            }

            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(self.name.clone())
        }
    }

    #[test]
    fn test_execution_planner_batches_parallel_and_sequential_calls() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(BatchTestTool::new(
            "read_a",
            ToolMetadata {
                read_only: true,
                concurrency_safe: true,
                risk: ToolRisk::Safe,
                max_result_chars: 100,
                exposed_by_default: true,
            },
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        )));
        registry.register(Box::new(BatchTestTool::new(
            "mutate",
            ToolMetadata {
                read_only: false,
                concurrency_safe: false,
                risk: ToolRisk::Mutating,
                max_result_chars: 100,
                exposed_by_default: true,
            },
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        )));
        registry.register(Box::new(BatchTestTool::new(
            "read_b",
            ToolMetadata {
                read_only: true,
                concurrency_safe: true,
                risk: ToolRisk::Safe,
                max_result_chars: 100,
                exposed_by_default: true,
            },
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        )));

        let plan = ExecutionPlanner::plan(
            &[
                ToolCall {
                    id: "1".to_string(),
                    name: "read_a".to_string(),
                    input: json!({}),
                },
                ToolCall {
                    id: "2".to_string(),
                    name: "mutate".to_string(),
                    input: json!({}),
                },
                ToolCall {
                    id: "3".to_string(),
                    name: "read_b".to_string(),
                    input: json!({}),
                },
            ],
            &registry,
        );

        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].kind, ExecutionBatchKind::ParallelReadOnly);
        assert_eq!(plan[1].kind, ExecutionBatchKind::SequentialMutating);
        assert_eq!(plan[2].kind, ExecutionBatchKind::ParallelReadOnly);
    }

    #[tokio::test]
    async fn test_subagent_dispatcher_runs_tasks_in_parallel() {
        // Three tasks, each with a read-only tool. With a cap of 3, all three
        // should be in-flight simultaneously.
        let barrier = Arc::new(Barrier::new(3));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let mut registry = ToolRegistry::new();
        for name in ["alpha", "beta", "gamma"] {
            registry.register(Box::new(
                BatchTestTool::new(
                    name,
                    ToolMetadata {
                        read_only: true,
                        concurrency_safe: true,
                        risk: ToolRisk::Safe,
                        max_result_chars: 100,
                        exposed_by_default: true,
                    },
                    Arc::clone(&active),
                    Arc::clone(&max_active),
                )
                .with_barrier(Arc::clone(&barrier)),
            ));
        }

        let dispatcher = SubagentDispatcher::new(3);
        let tasks = vec![
            SubagentTask {
                description: "task-a".to_string(),
                calls: vec![ToolCall {
                    id: "1".to_string(),
                    name: "alpha".to_string(),
                    input: json!({}),
                }],
            },
            SubagentTask {
                description: "task-b".to_string(),
                calls: vec![ToolCall {
                    id: "2".to_string(),
                    name: "beta".to_string(),
                    input: json!({}),
                }],
            },
            SubagentTask {
                description: "task-c".to_string(),
                calls: vec![ToolCall {
                    id: "3".to_string(),
                    name: "gamma".to_string(),
                    input: json!({}),
                }],
            },
        ];

        let results = dispatcher.dispatch(&registry, tasks).await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].description, "task-a");
        assert_eq!(results[2].description, "task-c");
        // All three tools hit the barrier together, so peak concurrency was 3.
        assert_eq!(max_active.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_subagent_dispatcher_respects_concurrency_cap() {
        // Four tasks, cap=2 — peak concurrency must never exceed 2.
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let mut registry = ToolRegistry::new();
        for name in ["t1", "t2", "t3", "t4"] {
            registry.register(Box::new(BatchTestTool::new(
                name,
                ToolMetadata {
                    read_only: true,
                    concurrency_safe: true,
                    risk: ToolRisk::Safe,
                    max_result_chars: 100,
                    exposed_by_default: true,
                },
                Arc::clone(&active),
                Arc::clone(&max_active),
            )));
        }

        let dispatcher = SubagentDispatcher::new(2);
        let tasks: Vec<SubagentTask> = ["t1", "t2", "t3", "t4"]
            .iter()
            .enumerate()
            .map(|(i, name)| SubagentTask {
                description: format!("task-{}", i),
                calls: vec![ToolCall {
                    id: i.to_string(),
                    name: name.to_string(),
                    input: json!({}),
                }],
            })
            .collect();

        let results = dispatcher.dispatch(&registry, tasks).await;

        assert_eq!(results.len(), 4);
        assert!(max_active.load(Ordering::SeqCst) <= 2);
        assert!(max_active.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn test_subagent_dispatcher_preserves_task_order() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(BatchTestTool::new(
            "echo",
            ToolMetadata {
                read_only: true,
                concurrency_safe: true,
                risk: ToolRisk::Safe,
                max_result_chars: 100,
                exposed_by_default: true,
            },
            active,
            max_active,
        )));

        let tasks: Vec<SubagentTask> = (0..5)
            .map(|i| SubagentTask {
                description: format!("desc-{}", i),
                calls: vec![ToolCall {
                    id: i.to_string(),
                    name: "echo".to_string(),
                    input: json!({}),
                }],
            })
            .collect();

        let results = SubagentDispatcher::new(8).dispatch(&registry, tasks).await;
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.description, format!("desc-{}", i));
        }
    }

    #[test]
    fn test_subagent_dispatcher_zero_cap_clamps_to_one() {
        assert_eq!(SubagentDispatcher::new(0).max_concurrent(), 1);
    }

    #[tokio::test]
    async fn test_execute_tool_batches_runs_parallel_read_only_calls_together() {
        let barrier = Arc::new(Barrier::new(2));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let read_a = BatchTestTool::new(
            "read_a",
            ToolMetadata {
                read_only: true,
                concurrency_safe: true,
                risk: ToolRisk::Safe,
                max_result_chars: 100,
                exposed_by_default: true,
            },
            Arc::clone(&active),
            Arc::clone(&max_active),
        )
        .with_barrier(Arc::clone(&barrier));

        let read_b = BatchTestTool::new(
            "read_b",
            ToolMetadata {
                read_only: true,
                concurrency_safe: true,
                risk: ToolRisk::Safe,
                max_result_chars: 100,
                exposed_by_default: true,
            },
            active,
            Arc::clone(&max_active),
        )
        .with_barrier(barrier);

        let mut registry = ToolRegistry::new();
        registry.register(Box::new(read_a));
        registry.register(Box::new(read_b));

        let results = execute_tool_batches(
            &registry,
            &[
                ToolCall {
                    id: "1".to_string(),
                    name: "read_a".to_string(),
                    input: json!({}),
                },
                ToolCall {
                    id: "2".to_string(),
                    name: "read_b".to_string(),
                    input: json!({}),
                },
            ],
        )
        .await;

        assert_eq!(results.len(), 2);
        assert!(max_active.load(Ordering::SeqCst) > 1);
    }
}
