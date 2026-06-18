# Claude-Code-Inspired Plan For Prompting, Tool Safety, And Orchestration

Date: 2026-03-31

## Goal

Adopt the strongest operational ideas from `claude-code` in three areas:

1. Prompt engineering
2. Tool usage safety
3. Orchestration

The objective is not to copy `claude-code` wholesale. The objective is to make `bxnode-bot` more reliable and controllable while keeping the current Rust architecture relatively simple.

## Why These Three Areas Matter Together

In `claude-code`, prompting, permissions, and orchestration are not separate concerns. They reinforce each other:

- The prompt teaches the model how to behave with tools.
- The tool layer constrains what the model can do.
- The orchestration layer determines how tool calls are exposed, executed, and fed back.

`bxnode-bot` already has a good base:

- a clean agent loop in `src/agent/execution.rs`
- provider-native tool calling in `src/providers/*`
- a pluggable context engine in `src/agent/context_engine.rs`
- working coding tools in `src/agent/coding_tools.rs`

The main weakness is that these pieces are still loosely connected.

## Current Gaps In BXNode Bot

### 1. Prompt Engineering Is Too Flat

The active coding prompt is a single string assembled in `src/gateway/mod.rs`. It mixes project info, tool list, and lightweight behavior rules in one place.

Current problems:

- there is no prompt section model
- there is no clear precedence order between base prompt, project prompt, skill instructions, environment facts, and custom instructions
- active skills are discoverable in the product, but are not clearly wired into the runtime prompt path
- the prompt still teaches XML-style manual tool calls, even though the provider layer already supports native tool calling

### 2. Tool Safety Is Mostly Post-Hoc

The tool layer validates tool names and sanitizes tool output, which is good, but the `Tool` trait does not expose safety semantics.

Current problems:

- no distinction between read-only and mutating tools
- no destructive-action classification
- no per-tool permission behavior
- no pre-exposure filtering of tools before they are shown to the model
- shell enablement is project-wide, but not policy-driven

### 3. Orchestration Is Structurally Too Simple

The current loop is easy to follow, but it lacks structured execution planning.

Current problems:

- `ToolRegistry::execute_all()` is always sequential
- tool results are flattened into plain text and reinserted as a user message
- the provider streaming abstraction is text-only, so native tool events get converted back into XML-ish text during streaming
- prompt assembly, tool execution, and context lifecycle are not coordinated by a single orchestration layer

## Design Principles For The Refactor

1. Keep provider support broad.
2. Prefer native tool calling wherever the provider supports it.
3. Fail closed on risky tools.
4. Keep the simple agent loop visible, but move policy and planning behind explicit types.
5. Make prompt construction composable and testable.
6. Preserve current working behavior during rollout with compatibility shims where needed.

## Proposed Architecture

### A. Prompt Layer

Introduce a prompt builder that assembles system instructions from typed sections instead of one formatted string.

Proposed new file:

- `src/agent/prompt.rs`

Proposed types:

```rust
pub enum PromptSectionKind {
    Identity,
    SystemRules,
    ToolUsage,
    ActionSafety,
    Environment,
    Project,
    Skills,
    Custom,
}

pub struct PromptSection {
    pub kind: PromptSectionKind,
    pub title: Option<String>,
    pub content: String,
    pub priority: u16,
}

pub struct PromptContext {
    pub model: String,
    pub project_name: Option<String>,
    pub workspace_dir: Option<std::path::PathBuf>,
    pub shell_enabled: bool,
    pub active_tool_names: Vec<String>,
    pub custom_instructions: Option<String>,
    pub active_skill_instructions: Option<String>,
}
```

Responsibilities:

- build a stable base prompt for coding tasks
- append environment facts separately
- inject project-specific custom instructions
- inject active skill instructions
- remove XML tool-call syntax from prompt guidance
- teach tool usage priorities such as "prefer dedicated tools over shell"

Prompt sections to add:

1. Identity
2. System rules
3. Tool usage rules
4. Action safety rules
5. Environment facts
6. Project context
7. Active skills
8. Project custom instructions

Prompt behavior to copy from `claude-code`:

- clear behavior guidance for tool choice
- short and explicit guidance about risky actions
- environment details added separately from base behavior
- session-conditional sections only when relevant

### B. Tool Safety Layer

Extend the tool contract with metadata and permission behavior.

Primary file to update:

- `src/agent/tools.rs`

Proposed types:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    Safe,
    Mutating,
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermissionMode {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone)]
pub struct ToolMetadata {
    pub read_only: bool,
    pub concurrency_safe: bool,
    pub risk: ToolRisk,
    pub max_result_chars: usize,
    pub exposed_by_default: bool,
}

#[derive(Debug, Clone)]
pub struct ToolPermissionDecision {
    pub mode: ToolPermissionMode,
    pub reason: Option<String>,
}
```

Extend the trait:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            read_only: false,
            concurrency_safe: false,
            risk: ToolRisk::Mutating,
            max_result_chars: 64_000,
            exposed_by_default: true,
        }
    }
    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String>;
}
```

Add a permission policy module:

- `src/agent/tool_policy.rs`

Proposed responsibilities:

- decide whether a tool is visible to the model
- decide whether a tool call may execute automatically
- apply project-specific policy for shell, delete, git mutation, and web/network tools
- allow future plugin tools to participate in the same policy system

Suggested configuration additions:

- `config.tool_permissions.default_mode`
- `project.allowed_tools`
- `project.denied_tools`
- `project.shell_policy.allowed_prefixes`
- `project.shell_policy.denied_prefixes`
- `project.confirm_destructive_tools`

### C. Orchestration Layer

Introduce an explicit orchestration module that plans and executes tool work based on metadata.

Proposed new file:

- `src/agent/orchestration.rs`

Proposed types:

```rust
pub enum ExecutionBatchKind {
    ParallelReadOnly,
    SequentialMutating,
}

pub struct ToolExecutionBatch {
    pub kind: ExecutionBatchKind,
    pub calls: Vec<ToolCall>,
}

pub struct ExecutionPlanner;

impl ExecutionPlanner {
    pub fn plan(calls: &[ToolCall], registry: &ToolRegistry) -> Vec<ToolExecutionBatch>;
}
```

Responsibilities:

- partition tool calls by safety class
- run read-only and concurrency-safe tools in parallel
- run mutating or destructive tools sequentially
- preserve result ordering for model feedback and UI events
- centralize cancellation and timeout behavior later

## The Real Orchestration Blocker: Provider Streaming Types

Today the streaming provider API returns text chunks only:

- `Provider::complete_stream(...) -> BoxStream<anyhow::Result<String>>`

That design forces provider implementations to convert structured native tool events into text payloads during streaming.

This is the main reason the code emits `<tool_call>...</tool_call>` XML in streaming paths.

Phase 3 should therefore modernize provider streaming first.

Primary file to update:

- `src/providers/mod.rs`

Proposed types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    TextDelta { content: String },
    ToolCall { id: String, name: String, arguments: serde_json::Value },
    Done,
}
```

Proposed provider trait change:

```rust
async fn complete_stream(
    &self,
    request: CompletionRequest,
) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>>;
```

This will allow:

- native tool events in streaming mode
- one orchestration path for streaming and non-streaming
- removal of XML tool-call fallback from normal prompt behavior

## Concrete Implementation Plan

### Phase 1: Prompt Architecture

Goal:
Replace the single coding prompt with a sectioned prompt builder.

Files:

- add `src/agent/prompt.rs`
- update `src/agent/mod.rs`
- update `src/gateway/mod.rs`
- optionally update `src/skills/registry.rs`

Steps:

1. Create `PromptSection` and `PromptContext`.
2. Move `build_coding_system_prompt()` logic out of `src/gateway/mod.rs`.
3. Build a base coding prompt from typed sections.
4. Inject project custom instructions as a separate section.
5. Inject active skill instructions from `SkillRegistry::get_active_instructions()`.
6. Add environment details such as workspace, shell availability, and project name as their own section.
7. Remove the XML tool-call instruction from the prompt.
8. Keep the output concise and behavior-focused.

Tests:

- prompt includes shell guidance only when shell is enabled
- prompt includes skill instructions only when active skills exist
- prompt assembly order is stable
- custom instructions append without overwriting base safety guidance

Definition of done:

- prompt building is no longer embedded in gateway message handling
- active skills participate in runtime prompting
- XML tool-call syntax is no longer taught in the system prompt

### Phase 2: Tool Safety Model

Goal:
Make safety policy explicit in the tool contract and execution path.

Files:

- update `src/agent/tools.rs`
- add `src/agent/tool_policy.rs`
- update `src/agent/coding_tools.rs`
- update `src/config/mod.rs`
- update `src/project/mod.rs`

Steps:

1. Add `ToolMetadata` and default metadata to the trait.
2. Mark existing tools as read-only, mutating, or destructive.
3. Add a `ToolPolicy` evaluator for exposure and execution decisions.
4. Filter tool definitions before sending them to the provider.
5. Apply policy checks before execution, not just after parsing.
6. Add command-prefix policy for `shell_exec`.
7. Keep current tool name validation and output sanitization as final guards.

Suggested initial metadata assignments:

- `file_read`, `list_directory`, `file_search`, `git_status`: read-only
- `file_edit`, `file_write`: mutating
- `file_delete`, destructive git operations, unsafe shell commands: destructive
- `memory_recall`: read-only
- `memory_store`, `memory_forget`: mutating

Tests:

- denied tools are not exposed to the model
- destructive tools require explicit approval mode
- shell policy blocks denied prefixes
- read-only tools remain available when mutation tools are restricted

Definition of done:

- tool exposure is policy-aware
- tool execution is policy-aware
- shell safety is more granular than a single boolean

### Phase 3: Provider And Orchestration Refactor

Goal:
Make execution planning and streaming tool use fully structured.

Files:

- update `src/providers/mod.rs`
- update `src/providers/anthropic.rs`
- update `src/providers/openai_compatible.rs`
- update `src/agent/execution.rs`
- add `src/agent/orchestration.rs`

Steps:

1. Replace text-only stream chunks with typed stream events.
2. Remove XML tool-call emission from streaming provider implementations.
3. Add an execution planner that batches calls by metadata.
4. Run read-only and concurrency-safe calls in parallel.
5. Run mutating and destructive calls sequentially.
6. Preserve deterministic result ordering.
7. Stop flattening tool results into a generic `"Tool Results:\n..."` block where possible.
8. Feed structured tool results back into context assembly.

Suggested execution rule:

- if all calls in a consecutive block are `read_only && concurrency_safe`, run them in parallel
- otherwise run one call at a time

Tests:

- streaming providers emit `ToolCall` events natively
- mixed read/write batches are partitioned correctly
- sequential mutation ordering is preserved
- parallel read-only execution does not reorder final result presentation

Definition of done:

- streaming no longer depends on XML tool-call text
- orchestration decisions are metadata-driven
- `execute()` and `execute_stream()` share the same planning logic

### Phase 4: Context And Feedback Integration

Goal:
Make prompting, tool execution, and context management reinforce each other.

Files:

- update `src/agent/context_engine.rs`
- update `src/agent/context.rs`
- update `src/agent/execution.rs`

Steps:

1. Let the context engine decide how tool results are represented back to the model.
2. Add optional summarization for large tool outputs.
3. Respect `max_result_chars` from `ToolMetadata`.
4. Add compact result previews for large shell and search results.
5. Keep recent critical tool results available while allowing older ones to be summarized.

Tests:

- oversized tool results are truncated or summarized deterministically
- context assembly preserves important recent tool outcomes
- compaction does not drop the prompt sections or recent safety-relevant events

Definition of done:

- tool results stop bloating context unnecessarily
- context compaction and tool orchestration are connected

## Suggested File-Level Ownership

### Prompt Work

- `src/agent/prompt.rs`
- `src/gateway/mod.rs`
- `src/skills/registry.rs`

### Tool Safety Work

- `src/agent/tools.rs`
- `src/agent/tool_policy.rs`
- `src/agent/coding_tools.rs`
- `src/config/mod.rs`
- `src/project/mod.rs`

### Orchestration Work

- `src/agent/orchestration.rs`
- `src/agent/execution.rs`
- `src/providers/mod.rs`
- `src/providers/anthropic.rs`
- `src/providers/openai_compatible.rs`

## Rollout Strategy

1. Land the prompt refactor first because it is low-risk and immediately useful.
2. Land tool metadata and policy second because orchestration depends on it.
3. Land the provider streaming refactor before parallel execution changes.
4. Land parallel batching only after stream events are typed and tested.
5. Land context feedback improvements last.

## Risks And Mitigations

### Risk: Provider regressions

Mitigation:

- keep non-streaming path working during the stream refactor
- add provider-focused tests for Anthropic and OpenAI-compatible tool calls

### Risk: Overcomplicating the prompt layer

Mitigation:

- keep prompt sections simple strings
- avoid feature-flag-heavy branching at first
- focus on stable section ordering and a small number of sections

### Risk: Safety rules blocking legitimate workflows

Mitigation:

- make policy configurable per project
- start with permissive defaults plus explicit destructive-tool gating
- log policy decisions for debugging

### Risk: Parallel tool execution causing nondeterminism

Mitigation:

- parallelize read-only tools only
- preserve final result ordering
- do not parallelize shell or file mutation tools initially

## Recommended Execution Order

1. Prompt builder extraction and skill injection
2. Tool metadata and policy framework
3. Provider stream event refactor
4. Orchestration planner and read-only parallelism
5. Context feedback and tool-result summarization

## Definition Of Done

1. Prompts are assembled from typed sections, not a single inline format string.
2. Active skills affect runtime prompting.
3. Tools carry safety metadata.
4. Tool exposure and execution follow policy.
5. Streaming providers emit structured tool events.
6. Read-only tools can execute in parallel safely.
7. XML tool-call prompting is removed from the normal runtime path.
