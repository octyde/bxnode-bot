//! User approval management for channel access control

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// A pending approval request from an unknown user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub user_id: String,
    pub user_name: Option<String>,
    pub channel: String,
    pub chat_id: String,
    pub first_message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Persisted approval state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApprovalState {
    /// Users approved at runtime: "channel:user_id" -> approval timestamp
    pub approved_users: HashMap<String, String>,
    /// Users rejected/blocked: "channel:user_id"
    pub rejected_users: HashSet<String>,
    /// Pending approval requests: id -> PendingApproval
    pub pending: HashMap<String, PendingApproval>,
}

/// Manages the approval queue and persists state to disk
pub struct ApprovalManager {
    state: Arc<RwLock<ApprovalState>>,
    store_path: String,
}

impl ApprovalManager {
    /// Create a new ApprovalManager, loading state from disk if available
    pub fn new(store_path: &str) -> Self {
        let state = if let Ok(data) = std::fs::read_to_string(store_path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            ApprovalState::default()
        };

        Self {
            state: Arc::new(RwLock::new(state)),
            store_path: store_path.to_string(),
        }
    }

    /// Check if a user has been approved for a channel
    pub async fn is_approved(&self, channel: &str, user_id: &str) -> bool {
        let key = format!("{}:{}", channel, user_id);
        let state = self.state.read().await;
        state.approved_users.contains_key(&key)
    }

    /// Check if a user has been rejected for a channel
    pub async fn is_rejected(&self, channel: &str, user_id: &str) -> bool {
        let key = format!("{}:{}", channel, user_id);
        let state = self.state.read().await;
        state.rejected_users.contains(&key)
    }

    /// Check if a user has a pending approval request
    pub async fn is_pending(&self, channel: &str, user_id: &str) -> bool {
        let state = self.state.read().await;
        state
            .pending
            .values()
            .any(|p| p.channel == channel && p.user_id == user_id)
    }

    /// Add a new pending approval request
    pub async fn add_pending(&self, approval: PendingApproval) -> String {
        let id = approval.id.clone();
        let mut state = self.state.write().await;
        state.pending.insert(id.clone(), approval);
        self.persist(&state);
        id
    }

    /// Approve a pending user. Returns the approval if found.
    pub async fn approve(&self, id: &str) -> Option<PendingApproval> {
        let mut state = self.state.write().await;
        if let Some(approval) = state.pending.remove(id) {
            let key = format!("{}:{}", approval.channel, approval.user_id);
            state
                .approved_users
                .insert(key, chrono::Utc::now().to_rfc3339());
            self.persist(&state);
            Some(approval)
        } else {
            None
        }
    }

    /// Reject a pending user. Returns the approval if found.
    pub async fn reject(&self, id: &str) -> Option<PendingApproval> {
        let mut state = self.state.write().await;
        if let Some(approval) = state.pending.remove(id) {
            let key = format!("{}:{}", approval.channel, approval.user_id);
            state.rejected_users.insert(key);
            self.persist(&state);
            Some(approval)
        } else {
            None
        }
    }

    /// List all pending approval requests
    pub async fn list_pending(&self) -> Vec<PendingApproval> {
        let state = self.state.read().await;
        state.pending.values().cloned().collect()
    }

    /// Persist state to disk
    fn persist(&self, state: &ApprovalState) {
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(&self.store_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(state) {
            let _ = std::fs::write(&self.store_path, json);
        }
    }
}
