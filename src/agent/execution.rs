//! Agent execution loop
//!
//! Handles the main agent loop: receive message, call LLM, execute tools, return response.

use std::sync::Arc;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use super::context::AgentContext;
use super::tools::{ToolCall, ToolRegistry, ToolResult};
use super::AgentConfig;
use crate::providers::{CompletionRequest, CompletionResponse, Provider};

/// Maximum number of tool call iterations
const MAX_TOOL_ITERATIONS: usize = 10;

/// Agent execution event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// Agent started processing
    #[serde(rename = "started")]
    Started { session_id: String },

    /// Streaming text chunk
    #[serde(rename = "text_delta")]
    TextDelta { content: String },

    /// Tool call initiated
    #[serde(rename = "tool_call")]
    ToolCall { call: ToolCall },

    /// Tool result received
    #[serde(rename = "tool_result")]
    ToolResult { result: ToolResult },

    /// Agent completed
    #[serde(rename = "completed")]
    Completed {
        content: String,
        usage: ExecutionUsage,
    },

    /// Error occurred
    #[serde(rename = "error")]
    Error { message: String },
}

/// Execution usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub tool_calls: usize,
    pub iterations: usize,
}

/// Agent executor
pub struct AgentExecutor {
    /// Provider for LLM calls
    provider: Arc<dyn Provider>,

    /// Tool registry
    tools: Arc<ToolRegistry>,

    /// Agent configuration
    config: AgentConfig,
}

impl AgentExecutor {
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        config: AgentConfig,
    ) -> Self {
        Self {
            provider,
            tools,
            config,
        }
    }

    /// Execute a single turn (non-streaming)
    pub async fn execute(
        &self,
        context: &mut AgentContext,
        user_message: &str,
    ) -> anyhow::Result<ExecutionResult> {
        // Add user message to context
        context.add_user_message(user_message);

        let mut usage = ExecutionUsage::default();
        let mut final_content = String::new();

        // Main execution loop (handles tool calls)
        for iteration in 0..MAX_TOOL_ITERATIONS {
            usage.iterations = iteration + 1;

            // Build completion request
            let request = CompletionRequest {
                model: self.config.model.clone(),
                messages: context.messages_for_provider(),
                temperature: self.config.temperature,
                max_tokens: Some(4096),
                stop: vec![],
                stream: false,
            };

            // Call the provider
            let response = self.provider.complete(request).await?;

            // Update usage
            usage.prompt_tokens += response.usage.prompt_tokens;
            usage.completion_tokens += response.usage.completion_tokens;
            usage.total_tokens += response.usage.total_tokens;

            // Check for tool calls in response
            let tool_calls = self.extract_tool_calls(&response);

            if tool_calls.is_empty() {
                // No tool calls, we're done
                final_content = response.content.clone();
                context.add_assistant_message(&response.content);
                break;
            }

            // Execute tool calls
            usage.tool_calls += tool_calls.len();
            let results = self.tools.execute_all(&tool_calls).await;

            // Add assistant message with tool calls indication
            context.add_assistant_message(&response.content);

            // Add tool results as user messages (simplified approach)
            let tool_results_text = self.format_tool_results(&results);
            context.add_user_message(tool_results_text);
        }

        Ok(ExecutionResult {
            content: final_content,
            usage,
        })
    }

    /// Execute with streaming
    pub async fn execute_stream<F>(
        &self,
        context: &mut AgentContext,
        user_message: &str,
        mut on_event: F,
    ) -> anyhow::Result<ExecutionResult>
    where
        F: FnMut(AgentEvent) + Send,
    {
        // Add user message to context
        context.add_user_message(user_message);

        on_event(AgentEvent::Started {
            session_id: "stream".to_string(),
        });

        let mut usage = ExecutionUsage::default();
        let mut final_content = String::new();

        // Main execution loop
        for iteration in 0..MAX_TOOL_ITERATIONS {
            usage.iterations = iteration + 1;

            // Build completion request
            let request = CompletionRequest {
                model: self.config.model.clone(),
                messages: context.messages_for_provider(),
                temperature: self.config.temperature,
                max_tokens: Some(4096),
                stop: vec![],
                stream: true,
            };

            // Call the provider with streaming
            let mut stream = self.provider.complete_stream(request).await?;

            let mut response_content = String::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        response_content.push_str(&chunk);
                        on_event(AgentEvent::TextDelta {
                            content: chunk,
                        });
                    }
                    Err(e) => {
                        on_event(AgentEvent::Error {
                            message: e.to_string(),
                        });
                        return Err(e);
                    }
                }
            }

            // Parse response for tool calls
            let tool_calls = self.parse_tool_calls_from_text(&response_content);

            if tool_calls.is_empty() {
                // No tool calls, we're done
                final_content = response_content.clone();
                context.add_assistant_message(&response_content);
                break;
            }

            // Execute tool calls
            usage.tool_calls += tool_calls.len();

            for call in &tool_calls {
                on_event(AgentEvent::ToolCall { call: call.clone() });
            }

            let results = self.tools.execute_all(&tool_calls).await;

            for result in &results {
                on_event(AgentEvent::ToolResult {
                    result: result.clone(),
                });
            }

            // Add to context
            context.add_assistant_message(&response_content);
            let tool_results_text = self.format_tool_results(&results);
            context.add_user_message(tool_results_text);
        }

        on_event(AgentEvent::Completed {
            content: final_content.clone(),
            usage: usage.clone(),
        });

        Ok(ExecutionResult {
            content: final_content,
            usage,
        })
    }

    /// Extract tool calls from a completion response
    fn extract_tool_calls(&self, _response: &CompletionResponse) -> Vec<ToolCall> {
        // TODO: Implement proper tool call extraction based on provider format
        // This is a placeholder - real implementation would parse the response
        // based on the provider's tool call format
        vec![]
    }

    /// Parse tool calls from response text (simple format)
    fn parse_tool_calls_from_text(&self, _text: &str) -> Vec<ToolCall> {
        // TODO: Implement text-based tool call parsing
        // This could look for patterns like <tool_call>...</tool_call>
        vec![]
    }

    /// Format tool results for context
    fn format_tool_results(&self, results: &[ToolResult]) -> String {
        let mut output = String::from("Tool Results:\n");

        for result in results {
            if result.success {
                output.push_str(&format!(
                    "[{}] Success: {}\n",
                    result.tool_call_id, result.content
                ));
            } else {
                output.push_str(&format!(
                    "[{}] Error: {}\n",
                    result.tool_call_id,
                    result.error.as_deref().unwrap_or("Unknown error")
                ));
            }
        }

        output
    }
}

/// Result of agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Final response content
    pub content: String,

    /// Usage statistics
    pub usage: ExecutionUsage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{FinishReason, ModelInfo, Usage};
    use async_trait::async_trait;
    use futures_util::stream::BoxStream;

    /// Mock provider for testing
    struct MockProvider {
        response: String,
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn id(&self) -> &str {
            "mock"
        }

        fn models(&self) -> Vec<ModelInfo> {
            vec![ModelInfo {
                id: "mock-model".to_string(),
                name: "Mock Model".to_string(),
                context_length: 4096,
                capabilities: vec![],
            }]
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> anyhow::Result<CompletionResponse> {
            Ok(CompletionResponse {
                content: self.response.clone(),
                model: "mock-model".to_string(),
                finish_reason: FinishReason::Stop,
                usage: Usage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                },
            })
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> anyhow::Result<BoxStream<'static, anyhow::Result<String>>> {
            let content = self.response.clone();
            let stream = futures_util::stream::once(async move { Ok(content) });
            Ok(Box::pin(stream))
        }
    }

    #[tokio::test]
    async fn test_agent_executor_basic() {
        let provider = Arc::new(MockProvider {
            response: "Hello! How can I help you?".to_string(),
        });
        let tools = Arc::new(ToolRegistry::new());
        let config = AgentConfig::default();

        let executor = AgentExecutor::new(provider, tools, config);
        let mut context = AgentContext::default();

        let result = executor.execute(&mut context, "Hi!").await.unwrap();

        assert_eq!(result.content, "Hello! How can I help you?");
        assert_eq!(result.usage.iterations, 1);
    }

    #[tokio::test]
    async fn test_agent_executor_streaming() {
        let provider = Arc::new(MockProvider {
            response: "Streaming response".to_string(),
        });
        let tools = Arc::new(ToolRegistry::new());
        let config = AgentConfig::default();

        let executor = AgentExecutor::new(provider, tools, config);
        let mut context = AgentContext::default();

        let mut events = Vec::new();
        let result = executor
            .execute_stream(&mut context, "Test", |event| {
                events.push(event);
            })
            .await
            .unwrap();

        assert!(!events.is_empty());
        assert!(matches!(events[0], AgentEvent::Started { .. }));
        assert!(matches!(events.last(), Some(AgentEvent::Completed { .. })));
        assert_eq!(result.content, "Streaming response");
    }

    #[test]
    fn test_execution_usage_default() {
        let usage = ExecutionUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.tool_calls, 0);
        assert_eq!(usage.iterations, 0);
    }

    #[test]
    fn test_format_tool_results() {
        let provider = Arc::new(MockProvider {
            response: String::new(),
        });
        let tools = Arc::new(ToolRegistry::new());
        let config = AgentConfig::default();

        let executor = AgentExecutor::new(provider, tools, config);

        let results = vec![
            ToolResult::success("call-1", "Result 1"),
            ToolResult::error("call-2", "Error message"),
        ];

        let formatted = executor.format_tool_results(&results);
        assert!(formatted.contains("call-1"));
        assert!(formatted.contains("Result 1"));
        assert!(formatted.contains("Error message"));
    }
}
