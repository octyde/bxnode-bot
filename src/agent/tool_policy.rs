//! Tool metadata and policy enforcement.

use std::collections::HashSet;

use super::tools::Tool;

/// Coarse tool risk classification for exposure and execution policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolRisk {
    Safe,
    #[default]
    Mutating,
    Destructive,
}

/// Runtime metadata that informs orchestration and safety policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolMetadata {
    /// Whether the tool only reads state and does not mutate it.
    pub read_only: bool,
    /// Whether the tool can safely run alongside other tool calls.
    pub concurrency_safe: bool,
    /// Coarse safety classification.
    pub risk: ToolRisk,
    /// Maximum number of characters returned to the model from this tool.
    pub max_result_chars: usize,
    /// Whether the tool should be exposed without an explicit allowlist entry.
    pub exposed_by_default: bool,
}

impl Default for ToolMetadata {
    fn default() -> Self {
        Self {
            read_only: false,
            concurrency_safe: false,
            risk: ToolRisk::Mutating,
            max_result_chars: 64_000,
            exposed_by_default: true,
        }
    }
}

impl ToolMetadata {
    /// A read-only, concurrency-safe, Safe-risk tool (file_read, list, search,
    /// git_status, memory_recall, …).
    pub fn read_only() -> Self {
        Self {
            read_only: true,
            concurrency_safe: true,
            risk: ToolRisk::Safe,
            ..Self::default()
        }
    }

    /// A destructive tool that a confirm-destructive policy will gate
    /// (file_delete, …).
    pub fn destructive() -> Self {
        Self {
            risk: ToolRisk::Destructive,
            ..Self::default()
        }
    }
}

/// Policy outcome for a tool under the current session/project configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermissionMode {
    Allow,
    RequireApproval,
    Deny,
}

/// Permission decision with a user-visible explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPermissionDecision {
    pub mode: ToolPermissionMode,
    pub reason: Option<String>,
}

impl ToolPermissionDecision {
    fn allow() -> Self {
        Self {
            mode: ToolPermissionMode::Allow,
            reason: None,
        }
    }

    fn require_approval(reason: impl Into<String>) -> Self {
        Self {
            mode: ToolPermissionMode::RequireApproval,
            reason: Some(reason.into()),
        }
    }

    fn deny(reason: impl Into<String>) -> Self {
        Self {
            mode: ToolPermissionMode::Deny,
            reason: Some(reason.into()),
        }
    }
}

/// Project/session policy for tool exposure and execution.
#[derive(Debug, Clone, Default)]
pub struct ToolPolicy {
    allowed_tools: Option<HashSet<String>>,
    denied_tools: HashSet<String>,
    confirm_destructive_tools: bool,
}

impl ToolPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = normalize_tools(tools);
        self
    }

    pub fn with_denied_tools(mut self, tools: Vec<String>) -> Self {
        self.denied_tools = normalize_tools(tools).unwrap_or_default();
        self
    }

    pub fn require_destructive_confirmation(mut self, enabled: bool) -> Self {
        self.confirm_destructive_tools = enabled;
        self
    }

    pub fn allowed_tools(&self) -> Option<&HashSet<String>> {
        self.allowed_tools.as_ref()
    }

    pub fn denied_tools(&self) -> &HashSet<String> {
        &self.denied_tools
    }

    pub fn confirm_destructive_tools(&self) -> bool {
        self.confirm_destructive_tools
    }

    pub fn decision_for(&self, tool: &dyn Tool) -> ToolPermissionDecision {
        let name = tool.name();
        let metadata = tool.metadata();
        let explicitly_allowed = self
            .allowed_tools
            .as_ref()
            .is_some_and(|allowed| allowed.contains(name));

        if self.denied_tools.contains(name) {
            return ToolPermissionDecision::deny(format!(
                "Tool '{}' is blocked by current tool policy.",
                name
            ));
        }

        if let Some(allowed) = &self.allowed_tools {
            if !allowed.contains(name) {
                return ToolPermissionDecision::deny(format!(
                    "Tool '{}' is not enabled by current tool policy.",
                    name
                ));
            }
        }

        if !metadata.exposed_by_default && !explicitly_allowed {
            return ToolPermissionDecision::deny(format!(
                "Tool '{}' is hidden by default and must be explicitly enabled.",
                name
            ));
        }

        if self.confirm_destructive_tools && metadata.risk == ToolRisk::Destructive {
            return ToolPermissionDecision::require_approval(format!(
                "Tool '{}' requires explicit approval because it is marked destructive.",
                name
            ));
        }

        ToolPermissionDecision::allow()
    }

    pub fn allows_definition(&self, tool: &dyn Tool) -> bool {
        self.decision_for(tool).mode == ToolPermissionMode::Allow
    }
}

fn normalize_tools(tools: Vec<String>) -> Option<HashSet<String>> {
    let normalized: HashSet<String> = tools
        .into_iter()
        .map(|tool| tool.trim().to_string())
        .filter(|tool| !tool.is_empty())
        .collect();

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}
