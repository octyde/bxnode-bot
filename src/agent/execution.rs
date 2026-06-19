//! Agent execution loop
//!
//! Handles the main agent loop: receive message, call LLM, execute tools, return response.

use std::sync::Arc;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use super::context::AgentContext;
use super::context_engine::{CompactionResult, ContextEngine, LegacyContextEngine, TurnInfo};
use super::tools::{ToolCall, ToolRegistry, ToolResult};
use super::AgentConfig;
use crate::providers::{
    CompletionRequest, CompletionResponse, Provider, StreamEvent, ToolDefinitionRequest,
};

/// Maximum number of tool-call iterations in one agent turn.
///
/// The loop spends one iteration per round-trip to the model. Real multi-step
/// work (explore → edit → build → fix) needs more than a handful, so the cap is
/// generous; the loop never *needs* to hit it because the last iteration forces
/// a tools-disabled final answer (see [`MAX_TOOL_ITERATIONS`] usage). A model
/// that only ever reads is caught earlier by the no-progress detector.
const MAX_TOOL_ITERATIONS: usize = 24;

/// How many consecutive identical tool calls (same name + same arguments) count
/// as "spinning" — at which point the loop injects a nudge to break the model
/// out of an unproductive read-loop (mirrors openclaw's generic-repeat
/// detector). Identical *reads* are the common GLM failure mode.
const SPIN_REPEAT_THRESHOLD: usize = 4;

/// A stable signature for one tool call (name + canonical arguments), used to
/// detect a model repeating the exact same call without making progress.
fn tool_call_signature(call: &ToolCall) -> String {
    // `serde_json::Value`'s Display is stable enough for equality of identical
    // inputs; object key order from the same model is consistent within a turn.
    format!("{}::{}", call.name, call.input)
}

/// The nudge injected when the model spins on identical tool calls — steer it to
/// stop gathering and act (or answer) instead.
const SPIN_NUDGE: &str = "You have repeated the same tool call several times \
without new information. Stop gathering context now. Either make the concrete \
change the task needs (edit a file), or, if you cannot, reply with a short \
plain-text summary of what you found and what you would do next. Do NOT call \
the same read-only tool again.";

/// The instruction prepended on the final forced turn (tools disabled) so the
/// model produces a useful answer instead of the loop ending in silence.
const FORCED_FINAL_PROMPT: &str = "You have reached the tool-use limit for this \
turn. Do not request any more tools. Using only what you already know, reply \
now with a concise plain-text answer: what you did or found, and the single \
most useful next step. This is your last message for this turn.";

/// Honest fallback shown to the user if the turn still produced no text after the
/// forced final turn — never return empty silence (mirrors openclaw's
/// incomplete-turn surface).
const EXHAUSTED_FALLBACK: &str = "⚠️ I used the whole tool budget for this turn \
without reaching a conclusion. Some tool actions may have run — please review \
before retrying, then send a narrower instruction.";

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

    /// Context engine for managing context lifecycle
    context_engine: Arc<dyn ContextEngine>,
}

impl AgentExecutor {
    pub fn new(provider: Arc<dyn Provider>, tools: Arc<ToolRegistry>, config: AgentConfig) -> Self {
        Self {
            provider,
            tools,
            config,
            context_engine: Arc::new(LegacyContextEngine),
        }
    }

    /// Create with a custom context engine
    pub fn with_context_engine(
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        config: AgentConfig,
        engine: Arc<dyn ContextEngine>,
    ) -> Self {
        Self {
            provider,
            tools,
            config,
            context_engine: engine,
        }
    }

    /// Get a clone of the provider Arc (for rebuilding with new tools).
    pub fn provider(&self) -> Arc<dyn Provider> {
        Arc::clone(&self.provider)
    }

    /// Build tool definitions for the completion request
    fn build_tool_definitions(&self) -> Vec<ToolDefinitionRequest> {
        let defs: Vec<ToolDefinitionRequest> = self
            .tools
            .list()
            .into_iter()
            .map(|t| ToolDefinitionRequest {
                name: t.name,
                description: t.description,
                input_schema: t.input_schema,
            })
            .collect();
        eprintln!(
            "[agent] built {} tool definitions: {:?}",
            defs.len(),
            defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>()
        );
        defs
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
        let mut total_compacted: usize = 0;
        let tool_defs = self.build_tool_definitions();

        let mut last_signatures: Vec<String> = Vec::new();
        let mut repeat_streak: usize = 0;

        // Main execution loop (handles tool calls)
        for iteration in 0..MAX_TOOL_ITERATIONS {
            usage.iterations = iteration + 1;

            // Compact context via engine
            let compaction = self.context_engine.compact(context).await;
            total_compacted += compaction.messages_removed;

            // Final iteration forces a tools-disabled answer (see streaming).
            let forced_final = iteration == MAX_TOOL_ITERATIONS - 1;
            if forced_final {
                context.add_user_message(FORCED_FINAL_PROMPT);
            }

            // Assemble messages via engine
            let turn = TurnInfo {
                model: self.config.model.clone(),
                user_prompt: user_message.to_string(),
                turn_number: iteration,
            };
            let messages = self.context_engine.assemble(context, &turn).await;

            // Build completion request (no tools on the forced-final turn).
            let request = CompletionRequest {
                model: self.config.model.clone(),
                messages,
                temperature: self.config.temperature,
                max_tokens: Some(self.config.max_output_tokens),
                stop: vec![],
                stream: false,
                tools: if forced_final {
                    Vec::new()
                } else {
                    tool_defs.clone()
                },
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
                // Post-turn hook
                self.context_engine
                    .after_turn(context, &response.content)
                    .await;
                break;
            }

            // Anti-spin: same calls as last round ⇒ count the streak.
            let signatures: Vec<String> = tool_calls.iter().map(tool_call_signature).collect();
            if !last_signatures.is_empty() && signatures == last_signatures {
                repeat_streak += 1;
            } else {
                repeat_streak = 0;
            }
            last_signatures = signatures;
            let nudge_now = repeat_streak + 1 >= SPIN_REPEAT_THRESHOLD;

            // Execute tool calls
            usage.tool_calls += tool_calls.len();
            let results = self.tools.execute_all(&tool_calls).await;

            // Add assistant message with tool calls indication
            context.add_assistant_message(&response.content);

            // Add tool results as user messages (simplified approach)
            let tool_results_text = self.format_tool_results(&results);
            context.add_user_message(tool_results_text);

            if nudge_now {
                context.add_user_message(SPIN_NUDGE);
                repeat_streak = 0;
            }
        }

        if final_content.trim().is_empty() {
            final_content = EXHAUSTED_FALLBACK.to_string();
            context.add_assistant_message(&final_content);
        }

        Ok(ExecutionResult {
            content: final_content,
            usage,
            messages_compacted: total_compacted,
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
        let mut total_compacted: usize = 0;
        let tool_defs = self.build_tool_definitions();

        // Anti-spin: track the previous tool-call signatures so we can detect
        // the model repeating identical calls without progress, and a running
        // count of consecutive repeats.
        let mut last_signatures: Vec<String> = Vec::new();
        let mut repeat_streak: usize = 0;

        // Main execution loop
        for iteration in 0..MAX_TOOL_ITERATIONS {
            usage.iterations = iteration + 1;

            // The final iteration is a FORCED ANSWER turn: disable tools so the
            // model must reply with text instead of requesting yet another tool,
            // guaranteeing the turn ends with something useful rather than
            // silence (the dead-end this fixes). On that turn we also nudge it.
            let forced_final = iteration == MAX_TOOL_ITERATIONS - 1;
            if forced_final {
                context.add_user_message(FORCED_FINAL_PROMPT);
            }

            // Compact context via engine
            let compaction = self.context_engine.compact(context).await;
            total_compacted += compaction.messages_removed;
            if compaction.messages_removed > 0 {
                eprintln!(
                    "[agent] iter {iteration} compaction removed {} message(s) to fit the token budget",
                    compaction.messages_removed
                );
            }

            // Assemble messages via engine
            let turn = TurnInfo {
                model: self.config.model.clone(),
                user_prompt: user_message.to_string(),
                turn_number: iteration,
            };
            let messages = self.context_engine.assemble(context, &turn).await;

            // Diagnostic: dump the exact message shape going to the provider so
            // a malformed tool round (assistant-with-tool_calls not answered by
            // matching tool messages) is visible in the console. A correct
            // second turn reads e.g. [system, user, assistant(tool_calls=[..]),
            // tool(answers=call_x) x N]; a count like 3 here means the tool
            // results never landed (a stale binary, pre tool-round fix).
            eprintln!(
                "[agent] iter {} assembled {} messages: [{}]",
                iteration,
                messages.len(),
                messages
                    .iter()
                    .map(|m| {
                        let role = format!("{:?}", m.role).to_lowercase();
                        if !m.tool_calls.is_empty() {
                            let ids: Vec<&str> =
                                m.tool_calls.iter().map(|c| c.id.as_str()).collect();
                            format!("{role}(tool_calls={:?})", ids)
                        } else if let Some(id) = &m.tool_call_id {
                            format!("{role}(answers={id})")
                        } else {
                            format!("{role}({}c)", m.content.len())
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            // Invariant: every assistant(tool_calls) must be answered by a
            // contiguous run of tool messages whose ids cover its tool_calls
            // 1:1 (no missing, no extra, no empty, no duplicate), and must
            // never be the final message. A violation is exactly what Z.AI/GLM
            // rejects as "messages parameter is illegal" — surface it loudly
            // here instead of as an opaque provider 400.
            if let Err(why) = validate_tool_round(&messages) {
                eprintln!("[agent] WARNING malformed tool round: {why}");
            }

            // Build completion request. On the forced-final turn, send NO tools
            // so the model cannot keep calling them and must produce text.
            let request = CompletionRequest {
                model: self.config.model.clone(),
                messages,
                temperature: self.config.temperature,
                max_tokens: Some(self.config.max_output_tokens),
                stop: vec![],
                stream: true,
                tools: if forced_final {
                    Vec::new()
                } else {
                    tool_defs.clone()
                },
            };

            // Call the provider with streaming
            let mut stream = self.provider.complete_stream(request).await?;

            let mut response_content = String::new();
            // Native tool calls surfaced by the provider this turn.
            let mut native_tool_calls: Vec<crate::providers::ToolCallResponse> = Vec::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(StreamEvent::TextDelta(content)) => {
                        response_content.push_str(&content);
                        on_event(AgentEvent::TextDelta { content });
                    }
                    Ok(StreamEvent::ToolCall(tc)) => {
                        native_tool_calls.push(tc);
                    }
                    Ok(StreamEvent::Done(_)) => {
                        // Terminal marker — keep draining in case the provider
                        // emits trailing events, but nothing more is expected.
                    }
                    Err(e) => {
                        on_event(AgentEvent::Error {
                            message: e.to_string(),
                        });
                        return Err(e);
                    }
                }
            }

            // Prefer native tool calls; fall back to XML-in-text only if the
            // provider surfaced none (legacy / non-tool-calling models).
            //
            // `from_native` decides how the round is RECORDED below. Native
            // calls carry real, model-issued ids and replay as a proper
            // assistant(tool_calls) + tool(result) round. Text-parsed calls
            // have *synthesized* ids (`tc_1`, …) the model never issued —
            // replaying those as a native tool round makes Z.AI/GLM reject the
            // next request ("messages parameter is illegal"), because a tool
            // message's tool_call_id must reference an id the model actually
            // produced. The fallback path therefore uses the legacy text round.
            let from_native = !native_tool_calls.is_empty();
            let tool_calls: Vec<ToolCall> = if from_native {
                native_tool_calls
                    .iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        input: tc.arguments.clone(),
                    })
                    .collect()
            } else {
                self.parse_tool_calls_from_text(&response_content)
            };
            eprintln!(
                "[agent] iteration {}: response {} chars, {} native + {} total tool_calls",
                iteration,
                response_content.len(),
                native_tool_calls.len(),
                tool_calls.len()
            );

            if tool_calls.is_empty() {
                // No tool calls, we're done
                final_content = response_content.clone();
                context.add_assistant_message(&response_content);
                // Post-turn hook
                self.context_engine
                    .after_turn(context, &response_content)
                    .await;
                break;
            }

            // Anti-spin: if this round's tool calls are identical to the last
            // round's (same names + args), the model is stuck re-reading. Count
            // the streak; once it crosses the threshold, inject a one-time nudge
            // to make it act or answer instead of looping (openclaw's
            // generic-repeat pattern, adapted).
            let signatures: Vec<String> = tool_calls.iter().map(tool_call_signature).collect();
            if !last_signatures.is_empty() && signatures == last_signatures {
                repeat_streak += 1;
            } else {
                repeat_streak = 0;
            }
            last_signatures = signatures;
            let nudge_now = repeat_streak + 1 >= SPIN_REPEAT_THRESHOLD;

            // Execute tool calls
            usage.tool_calls += tool_calls.len();

            for call in &tool_calls {
                on_event(AgentEvent::ToolCall { call: call.clone() });
                // Supervision visibility: log each tool call's name + a short
                // arg preview so a stalled agent can be diagnosed (e.g. files
                // not landing because a shell command is malformed). Gated on
                // BXNODE_TOOL_TRACE=1 so normal runs stay quiet.
                if std::env::var("BXNODE_TOOL_TRACE").as_deref() == Ok("1") {
                    let args = call.input.to_string();
                    let preview: String = args.chars().take(220).collect();
                    eprintln!("[tooltrace] CALL {} args={}", call.name, preview);
                }
            }

            let results = self.tools.execute_all(&tool_calls).await;

            for result in &results {
                if std::env::var("BXNODE_TOOL_TRACE").as_deref() == Ok("1") {
                    let preview: String = result.content.chars().take(280).collect();
                    eprintln!(
                        "[tooltrace] RESULT call={} {}{}",
                        result.tool_call_id,
                        if result.success { "" } else { "ERROR " },
                        preview
                    );
                }
                on_event(AgentEvent::ToolResult {
                    result: result.clone(),
                });
            }

            if from_native {
                // Record a valid native tool round: an assistant message
                // carrying the tool_calls, followed by one tool-role message
                // per result keyed by tool_call_id. This is the message shape
                // OpenAI/Z.AI require — the old "assistant text + fake user
                // message" path is what triggered "messages parameter is
                // illegal". Every id here is a real, model-issued id.
                let assistant_tool_calls: Vec<crate::providers::ToolCallResponse> = tool_calls
                    .iter()
                    .map(|c| crate::providers::ToolCallResponse {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        arguments: c.input.clone(),
                    })
                    .collect();
                context.add_assistant_tool_calls(response_content.clone(), assistant_tool_calls);
                for result in &results {
                    context.add_tool_result(result.tool_call_id.clone(), result.content.clone());
                }
            } else {
                // Text-parsed (XML) tool calls have synthesized ids, not
                // model-issued ones. Replaying them as a native tool_calls
                // round would be rejected by Z.AI/GLM. Use the legacy text
                // round — assistant text + a user "Tool Results" message — so
                // no fabricated tool_call_id ever reaches the provider.
                context.add_assistant_message(&response_content);
                let tool_results_text = self.format_tool_results(&results);
                context.add_user_message(tool_results_text);
            }

            // Break a detected spin: append the nudge as a user message so the
            // model sees it on the next round. Done after the tool round is
            // recorded so the assistant(tool_calls)→tool(results) pairing the
            // provider requires is never broken (GLM "messages illegal" guard).
            if nudge_now {
                eprintln!(
                    "[agent] no-progress: {} identical tool round(s) — nudging the model to act",
                    repeat_streak + 1
                );
                context.add_user_message(SPIN_NUDGE);
                repeat_streak = 0;
            }
        }

        // The loop ended. If it ran to the cap without the model producing any
        // text (the forced-final turn still returned nothing, or every turn was
        // tool calls), surface an honest message instead of empty silence — the
        // chat must never look dead (the bug this fixes).
        if final_content.trim().is_empty() {
            eprintln!("[agent] turn exhausted the tool budget with no final text — using fallback");
            final_content = EXHAUSTED_FALLBACK.to_string();
            context.add_assistant_message(&final_content);
            on_event(AgentEvent::TextDelta {
                content: final_content.clone(),
            });
        }

        on_event(AgentEvent::Completed {
            content: final_content.clone(),
            usage: usage.clone(),
        });

        Ok(ExecutionResult {
            content: final_content,
            usage,
            messages_compacted: total_compacted,
        })
    }

    /// Extract tool calls from a completion response (native tool calling)
    fn extract_tool_calls(&self, response: &CompletionResponse) -> Vec<ToolCall> {
        let mut calls = Vec::new();

        // Extract from provider-native tool_calls field
        for tc in &response.tool_calls {
            calls.push(ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.arguments.clone(),
            });
        }

        // Also try parsing from response text as fallback
        if calls.is_empty() {
            calls = self.parse_tool_calls_from_text(&response.content);
        }

        calls
    }

    /// Parse tool calls from response text
    ///
    /// Supports XML-style: `<tool_call>{"name":"...","arguments":{...}}</tool_call>`
    fn parse_tool_calls_from_text(&self, text: &str) -> Vec<ToolCall> {
        let mut calls = Vec::new();
        let mut counter = 0u32;

        // Parse <tool_call>...</tool_call> blocks
        let mut remaining = text;
        while let Some(start) = remaining.find("<tool_call>") {
            let after_tag = &remaining[start + 11..];
            if let Some(end) = after_tag.find("</tool_call>") {
                let json_str = after_tag[..end].trim();
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let name = parsed
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = parsed
                        .get("arguments")
                        .or_else(|| parsed.get("input"))
                        .cloned()
                        .unwrap_or(serde_json::json!({}));

                    if !name.is_empty() {
                        counter += 1;
                        calls.push(ToolCall {
                            id: format!("tc_{}", counter),
                            name,
                            input: arguments,
                        });
                    }
                }
                remaining = &after_tag[end + 12..];
            } else {
                break;
            }
        }

        calls
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

/// Validate that a message array forms a legal tool round for an
/// OpenAI/Z.AI `/chat/completions` request.
///
/// Each assistant message carrying `tool_calls` must be followed immediately
/// by exactly one `tool` message per call id, covering the assistant's id set
/// 1:1 — no missing, no extra, no empty, no duplicate ids — and must never be
/// the final message in the array. These are precisely the constraints whose
/// violation Z.AI/GLM reports as "messages parameter is illegal".
fn validate_tool_round(messages: &[crate::providers::Message]) -> Result<(), String> {
    use crate::providers::Role;
    let mut i = 0;
    while i < messages.len() {
        let m = &messages[i];
        if m.role == Role::Assistant && !m.tool_calls.is_empty() {
            // Collect the assistant's call ids (must be non-empty + unique).
            let mut expected: Vec<&str> = Vec::with_capacity(m.tool_calls.len());
            for c in &m.tool_calls {
                if c.id.is_empty() {
                    return Err(format!(
                        "assistant message #{i} has a tool_call with an empty id"
                    ));
                }
                if expected.contains(&c.id.as_str()) {
                    return Err(format!(
                        "assistant message #{i} has duplicate tool_call id '{}'",
                        c.id
                    ));
                }
                expected.push(&c.id);
            }
            // The following messages must be exactly the matching tool results.
            for (k, want) in expected.iter().enumerate() {
                let answer = messages.get(i + 1 + k);
                match answer {
                    Some(a) if a.role == Role::Tool => {
                        match a.tool_call_id.as_deref() {
                            Some(got) if got == *want => {}
                            Some(got) => {
                                return Err(format!(
                                    "tool result #{} answers '{got}' but expected '{want}'",
                                    i + 1 + k
                                ))
                            }
                            None => {
                                return Err(format!(
                                    "tool message #{} is missing tool_call_id (expected '{want}')",
                                    i + 1 + k
                                ))
                            }
                        }
                    }
                    Some(a) => {
                        return Err(format!(
                            "assistant tool_calls at #{i} not answered: message #{} is {:?}, expected a tool result for '{want}'",
                            i + 1 + k,
                            a.role
                        ))
                    }
                    None => {
                        return Err(format!(
                            "assistant tool_calls at #{i} is the last message (or short {} results); expected a tool result for '{want}'",
                            expected.len()
                        ))
                    }
                }
            }
            i += 1 + expected.len();
        } else {
            i += 1;
        }
    }
    Ok(())
}

/// Result of agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Final response content
    pub content: String,

    /// Usage statistics
    pub usage: ExecutionUsage,

    /// Number of messages removed by context compaction (0 = no compaction)
    pub messages_compacted: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::Tool;
    use crate::providers::{FinishReason, ModelInfo, Role, Usage};
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
                tool_calls: vec![],
            })
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
            let content = self.response.clone();
            let stream = futures_util::stream::iter(vec![
                Ok(StreamEvent::TextDelta(content)),
                Ok(StreamEvent::Done(Some(FinishReason::Stop))),
            ]);
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

    /// A provider that emits a native tool call on the first turn, then plain
    /// text on the second — exercising the assistant(tool_calls) → tool(result)
    /// → assistant message round.
    struct ToolCallingMockProvider {
        turn: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl Provider for ToolCallingMockProvider {
        fn id(&self) -> &str {
            "mock-tools"
        }
        fn models(&self) -> Vec<ModelInfo> {
            vec![]
        }
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> anyhow::Result<CompletionResponse> {
            unreachable!("streaming-only test")
        }
        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
            use std::sync::atomic::Ordering;
            let n = self.turn.fetch_add(1, Ordering::SeqCst);
            let events: Vec<anyhow::Result<StreamEvent>> = if n == 0 {
                vec![
                    Ok(StreamEvent::ToolCall(crate::providers::ToolCallResponse {
                        id: "call_abc".to_string(),
                        name: "echo".to_string(),
                        arguments: serde_json::json!({"text": "hi"}),
                    })),
                    Ok(StreamEvent::Done(Some(FinishReason::ToolUse))),
                ]
            } else {
                vec![
                    Ok(StreamEvent::TextDelta("All done.".to_string())),
                    Ok(StreamEvent::Done(Some(FinishReason::Stop))),
                ]
            };
            Ok(Box::pin(futures_util::stream::iter(events)))
        }
    }

    /// Minimal echo tool that returns its `text` argument.
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes the input text"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}})
        }
        async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
            Ok(input
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string())
        }
    }

    #[tokio::test]
    async fn test_native_tool_call_round_builds_valid_messages() {
        let provider = Arc::new(ToolCallingMockProvider {
            turn: std::sync::atomic::AtomicUsize::new(0),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let tools = Arc::new(registry);
        let config = AgentConfig::default();

        let executor = AgentExecutor::new(provider, tools, config);
        let mut context = AgentContext::default();

        let mut events = Vec::new();
        let result = executor
            .execute_stream(&mut context, "echo hi", |e| events.push(e))
            .await
            .unwrap();

        assert_eq!(result.content, "All done.");

        // The recorded conversation must be a valid OpenAI/Z.AI tool round:
        // assistant(with tool_calls) → tool(result keyed by id) → assistant(text).
        let msgs = context.messages();
        let assistant_with_calls = msgs
            .iter()
            .find(|m| m.role == Role::Assistant && !m.tool_calls.is_empty())
            .expect("an assistant message carrying tool_calls");
        assert_eq!(assistant_with_calls.tool_calls[0].id, "call_abc");
        assert_eq!(assistant_with_calls.tool_calls[0].name, "echo");

        let tool_msg = msgs
            .iter()
            .find(|m| m.role == Role::Tool)
            .expect("a tool-role result message");
        assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call_abc"));
        assert_eq!(tool_msg.content, "hi");

        // And the model's native tool call surfaced as an AgentEvent::ToolCall.
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolCall { call } if call.id == "call_abc")));
    }

    /// A provider that requests the SAME tool call on every turn — until it is
    /// sent a request with NO tools (the forced-final turn), where it returns
    /// text. Models the GLM "reads forever" failure and proves the forced-final
    /// turn rescues it. Also records every message shape it is sent so a test
    /// can assert the tool-round contract is never violated.
    struct SpinningMockProvider {
        sent_message_shapes: std::sync::Mutex<Vec<usize>>,
        forced_turn_had_no_tools: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl Provider for SpinningMockProvider {
        fn id(&self) -> &str {
            "mock-spin"
        }
        fn models(&self) -> Vec<ModelInfo> {
            vec![]
        }
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> anyhow::Result<CompletionResponse> {
            unreachable!("streaming-only test")
        }
        async fn complete_stream(
            &self,
            request: CompletionRequest,
        ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
            use std::sync::atomic::Ordering;
            self.sent_message_shapes
                .lock()
                .unwrap()
                .push(request.messages.len());
            // The forced-final turn arrives with tools disabled → answer.
            if request.tools.is_empty() {
                self.forced_turn_had_no_tools.store(true, Ordering::SeqCst);
                return Ok(Box::pin(futures_util::stream::iter(vec![
                    Ok(StreamEvent::TextDelta(
                        "Here is what I found and the next step.".to_string(),
                    )),
                    Ok(StreamEvent::Done(Some(FinishReason::Stop))),
                ])));
            }
            // Otherwise: the same read forever.
            Ok(Box::pin(futures_util::stream::iter(vec![
                Ok(StreamEvent::ToolCall(crate::providers::ToolCallResponse {
                    id: format!("call_{}", request.messages.len()),
                    name: "echo".to_string(),
                    arguments: serde_json::json!({"text": "same"}),
                })),
                Ok(StreamEvent::Done(Some(FinishReason::ToolUse))),
            ])))
        }
    }

    #[tokio::test]
    async fn forced_final_turn_rescues_a_spinning_model() {
        // A model that only ever reads must NOT dead-end: the last iteration
        // disables tools and forces a text answer, so the turn returns content.
        let provider = Arc::new(SpinningMockProvider {
            sent_message_shapes: std::sync::Mutex::new(Vec::new()),
            forced_turn_had_no_tools: std::sync::atomic::AtomicBool::new(false),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let executor =
            AgentExecutor::new(provider.clone(), Arc::new(registry), AgentConfig::default());
        let mut context = AgentContext::default();

        let result = executor
            .execute_stream(&mut context, "do it", |_| {})
            .await
            .unwrap();

        // The turn ends with the forced text answer, not silence.
        assert!(
            !result.content.trim().is_empty(),
            "forced-final turn must produce text"
        );
        assert!(
            result.content.contains("next step"),
            "got the forced answer: {}",
            result.content
        );
        assert!(
            provider
                .forced_turn_had_no_tools
                .load(std::sync::atomic::Ordering::SeqCst),
            "the final turn must be sent with tools disabled"
        );
        // It used the whole budget (the model never stopped on its own).
        assert_eq!(result.usage.iterations, MAX_TOOL_ITERATIONS);

        // CRITICAL (GLM contract): despite the forced-final prompt + anti-spin
        // nudges being injected, the recorded conversation is still a valid tool
        // round — every assistant(tool_calls) is answered by its tool results,
        // contiguously. A violation is what GLM rejects as "messages illegal".
        assert!(
            validate_tool_round(context.messages()).is_ok(),
            "injected prompts must not break the tool-round contract: {:?}",
            validate_tool_round(context.messages())
        );
    }

    /// A provider that ALWAYS requests a tool — even when tools are disabled it
    /// returns another tool call (a degenerate model). Proves the exhaustion
    /// fallback fires so the user still gets an honest message, never silence.
    struct NeverAnswersMockProvider;

    #[async_trait]
    impl Provider for NeverAnswersMockProvider {
        fn id(&self) -> &str {
            "mock-never"
        }
        fn models(&self) -> Vec<ModelInfo> {
            vec![]
        }
        async fn complete(&self, _r: CompletionRequest) -> anyhow::Result<CompletionResponse> {
            unreachable!()
        }
        async fn complete_stream(
            &self,
            request: CompletionRequest,
        ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
            // Even on the forced (tools-disabled) turn, emit no text — the
            // worst case the fallback must handle.
            Ok(Box::pin(futures_util::stream::iter(vec![
                Ok(StreamEvent::ToolCall(crate::providers::ToolCallResponse {
                    id: format!("c{}", request.messages.len()),
                    name: "echo".to_string(),
                    arguments: serde_json::json!({"text": "x"}),
                })),
                Ok(StreamEvent::Done(Some(FinishReason::ToolUse))),
            ])))
        }
    }

    #[tokio::test]
    async fn exhaustion_fallback_prevents_empty_silence() {
        // Even if the forced-final turn yields nothing, the user gets an honest
        // message, never an empty result (the dead-looking-chat bug).
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let executor = AgentExecutor::new(
            Arc::new(NeverAnswersMockProvider),
            Arc::new(registry),
            AgentConfig::default(),
        );
        let mut context = AgentContext::default();
        let result = executor
            .execute_stream(&mut context, "go", |_| {})
            .await
            .unwrap();
        assert!(!result.content.trim().is_empty(), "never empty");
        assert_eq!(result.content, EXHAUSTED_FALLBACK);
    }

    #[test]
    fn test_validate_tool_round_accepts_a_well_formed_round() {
        use crate::providers::{Message, Role, ToolCallResponse};
        let msgs = vec![
            Message::text(Role::System, "sys"),
            Message::text(Role::User, "hi"),
            Message::assistant_tool_calls(
                "",
                vec![
                    ToolCallResponse {
                        id: "a".into(),
                        name: "x".into(),
                        arguments: serde_json::json!({}),
                    },
                    ToolCallResponse {
                        id: "b".into(),
                        name: "y".into(),
                        arguments: serde_json::json!({}),
                    },
                ],
            ),
            Message::tool_result("a", "ra"),
            Message::tool_result("b", "rb"),
            Message::text(Role::Assistant, "done"),
        ];
        assert!(validate_tool_round(&msgs).is_ok());
    }

    #[test]
    fn test_validate_tool_round_rejects_orphan_and_mismatch() {
        use crate::providers::{Message, Role, ToolCallResponse};
        // Orphan: assistant tool_calls is the last message.
        let orphan = vec![
            Message::text(Role::User, "hi"),
            Message::assistant_tool_calls(
                "",
                vec![ToolCallResponse {
                    id: "a".into(),
                    name: "x".into(),
                    arguments: serde_json::json!({}),
                }],
            ),
        ];
        assert!(validate_tool_round(&orphan).is_err());

        // Mismatch: the tool result answers a different id.
        let mismatch = vec![
            Message::assistant_tool_calls(
                "",
                vec![ToolCallResponse {
                    id: "a".into(),
                    name: "x".into(),
                    arguments: serde_json::json!({}),
                }],
            ),
            Message::tool_result("WRONG", "r"),
        ];
        assert!(validate_tool_round(&mismatch).is_err());

        // Empty id is illegal.
        let empty_id = vec![
            Message::assistant_tool_calls(
                "",
                vec![ToolCallResponse {
                    id: "".into(),
                    name: "x".into(),
                    arguments: serde_json::json!({}),
                }],
            ),
            Message::tool_result("", "r"),
        ];
        assert!(validate_tool_round(&empty_id).is_err());
    }

    /// A provider that emits NO native tool calls, instead returning an
    /// XML `<tool_call>` block as plain text on the first turn (the legacy
    /// shape some GLM builds fall back to), then plain text on the second.
    struct XmlToolCallMockProvider {
        turn: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl Provider for XmlToolCallMockProvider {
        fn id(&self) -> &str {
            "mock-xml"
        }
        fn models(&self) -> Vec<ModelInfo> {
            vec![]
        }
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> anyhow::Result<CompletionResponse> {
            unreachable!("streaming-only test")
        }
        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
            use std::sync::atomic::Ordering;
            let n = self.turn.fetch_add(1, Ordering::SeqCst);
            let events: Vec<anyhow::Result<StreamEvent>> = if n == 0 {
                vec![
                    Ok(StreamEvent::TextDelta(
                        "<tool_call>{\"name\":\"echo\",\"arguments\":{\"text\":\"hi\"}}</tool_call>"
                            .to_string(),
                    )),
                    Ok(StreamEvent::Done(Some(FinishReason::Stop))),
                ]
            } else {
                vec![
                    Ok(StreamEvent::TextDelta("All done.".to_string())),
                    Ok(StreamEvent::Done(Some(FinishReason::Stop))),
                ]
            };
            Ok(Box::pin(futures_util::stream::iter(events)))
        }
    }

    #[tokio::test]
    async fn test_text_fallback_tool_round_never_sends_synthetic_ids() {
        let provider = Arc::new(XmlToolCallMockProvider {
            turn: std::sync::atomic::AtomicUsize::new(0),
        });
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let tools = Arc::new(registry);
        let config = AgentConfig::default();

        let executor = AgentExecutor::new(provider, tools, config);
        let mut context = AgentContext::default();

        let result = executor
            .execute_stream(&mut context, "echo hi", |_| {})
            .await
            .unwrap();
        assert_eq!(result.content, "All done.");

        // The XML fallback path must NOT emit a native tool round: synthesized
        // ids (tc_1, …) the model never issued would make Z.AI/GLM reject the
        // next request. So there must be NO Role::Tool message and NO assistant
        // message carrying tool_calls — the round is recorded as the legacy
        // assistant-text + user "Tool Results" pair instead.
        let msgs = context.messages();
        assert!(
            !msgs.iter().any(|m| m.role == Role::Tool),
            "text-fallback must not produce role:tool messages with fabricated ids"
        );
        assert!(
            !msgs.iter().any(|m| !m.tool_calls.is_empty()),
            "text-fallback must not produce an assistant message with tool_calls"
        );
        // The tool actually ran and its result was threaded back as user text.
        assert!(
            msgs.iter()
                .any(|m| m.role == Role::User && m.content.contains("hi")),
            "the echo result should appear in a user 'Tool Results' message"
        );
    }
}
