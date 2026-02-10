//! BXNode Bot Desktop - Tauri Application
//!
//! Native desktop application for managing BXNode Bot skills and server.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::RwLock;

use bxnode_bot::config::Config;
use bxnode_bot::skills::{SkillInfo, SkillRegistry, SkillSyncer, SyncReport};

/// Application state shared across Tauri commands
pub struct AppState {
    pub skills: Arc<RwLock<Option<SkillRegistry>>>,
    pub config: Arc<RwLock<Config>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            skills: Arc::new(RwLock::new(None)),
            config: Arc::new(RwLock::new(Config::default())),
        }
    }
}

/// Skill detail response including instructions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetail {
    #[serde(flatten)]
    pub info: SkillInfo,
    pub instructions: Option<String>,
}

/// Sync report response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub success: bool,
    pub synced: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<(String, String)>,
    pub total: usize,
}

impl From<SyncReport> for SyncResponse {
    fn from(report: SyncReport) -> Self {
        let total = report.total();
        Self {
            success: report.is_success(),
            synced: report.synced,
            skipped: report.skipped,
            errors: report.errors,
            total,
        }
    }
}

/// Skills configuration summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfigSummary {
    pub enabled: bool,
    pub total_skills: usize,
    pub active_skills: usize,
    pub directories: Vec<String>,
}

/// Stats response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub version: String,
    pub providers: usize,
    pub skills_enabled: bool,
    pub total_skills: usize,
    pub active_skills: usize,
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Get application status
#[tauri::command]
fn get_status() -> String {
    serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    })
    .to_string()
}

/// Get application stats
#[tauri::command]
async fn get_stats(state: State<'_, AppState>) -> Result<StatsResponse, String> {
    let skills = state.skills.read().await;

    let (total_skills, active_skills) = if let Some(ref registry) = *skills {
        (registry.count(), registry.active_count())
    } else {
        (0, 0)
    };

    Ok(StatsResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        providers: 0, // TODO: Get from provider registry
        skills_enabled: skills.is_some(),
        total_skills,
        active_skills,
    })
}

/// Initialize the skills system
#[tauri::command]
async fn init_skills(state: State<'_, AppState>) -> Result<SkillsConfigSummary, String> {
    let config = state.config.read().await;

    if !config.skills.enabled {
        return Ok(SkillsConfigSummary {
            enabled: false,
            total_skills: 0,
            active_skills: 0,
            directories: vec![],
        });
    }

    let registry = SkillRegistry::from_config(&config.skills).map_err(|e| e.to_string())?;

    let summary = SkillsConfigSummary {
        enabled: true,
        total_skills: registry.count(),
        active_skills: registry.active_count(),
        directories: registry
            .directories()
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    };

    let mut skills = state.skills.write().await;
    *skills = Some(registry);

    Ok(summary)
}

/// List all skills
#[tauri::command]
async fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, String> {
    let skills = state.skills.read().await;

    match &*skills {
        Some(registry) => Ok(registry.list_info()),
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Get skill details by name
#[tauri::command]
async fn get_skill(name: String, state: State<'_, AppState>) -> Result<SkillDetail, String> {
    let skills = state.skills.read().await;

    match &*skills {
        Some(registry) => match registry.get(&name) {
            Some(skill_ref) => {
                let mut info = SkillInfo::from(skill_ref);
                info.is_active = registry.is_active(&name);

                Ok(SkillDetail {
                    info,
                    instructions: skill_ref.instructions().map(|s| s.to_string()),
                })
            }
            None => Err(format!("Skill '{}' not found", name)),
        },
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Enable a skill
#[tauri::command]
async fn enable_skill(name: String, state: State<'_, AppState>) -> Result<String, String> {
    let mut skills = state.skills.write().await;

    match &mut *skills {
        Some(registry) => {
            registry.enable(&name).map_err(|e| e.to_string())?;
            Ok(format!("Skill '{}' enabled", name))
        }
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Disable a skill
#[tauri::command]
async fn disable_skill(name: String, state: State<'_, AppState>) -> Result<String, String> {
    let mut skills = state.skills.write().await;

    match &mut *skills {
        Some(registry) => {
            registry.disable(&name);
            Ok(format!("Skill '{}' disabled", name))
        }
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Sync skills from awesome-openclaw-skills
#[tauri::command]
async fn sync_skills(state: State<'_, AppState>) -> Result<SyncResponse, String> {
    let skills = state.skills.read().await;

    let directories = match &*skills {
        Some(registry) => registry.directories().to_vec(),
        None => return Err("Skills system not initialized".to_string()),
    };

    if directories.is_empty() {
        return Err("No skill directories configured".to_string());
    }

    // Use the first directory as target
    let target_dir = directories[0].clone();
    drop(skills); // Release read lock

    let syncer = SkillSyncer::new(target_dir);

    let report = syncer
        .sync_from_awesome_list(false)
        .await
        .map_err(|e| e.to_string())?;

    // Rescan skills after sync
    let mut skills = state.skills.write().await;
    if let Some(ref mut registry) = *skills {
        let _ = registry.scan_all();
    }

    Ok(SyncResponse::from(report))
}

/// Get skills configuration summary
#[tauri::command]
async fn get_skills_config(state: State<'_, AppState>) -> Result<SkillsConfigSummary, String> {
    let skills = state.skills.read().await;

    match &*skills {
        Some(registry) => Ok(SkillsConfigSummary {
            enabled: true,
            total_skills: registry.count(),
            active_skills: registry.active_count(),
            directories: registry
                .directories()
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        }),
        None => Ok(SkillsConfigSummary {
            enabled: false,
            total_skills: 0,
            active_skills: 0,
            directories: vec![],
        }),
    }
}

/// Refresh/rescan skills
#[tauri::command]
async fn refresh_skills(state: State<'_, AppState>) -> Result<usize, String> {
    let mut skills = state.skills.write().await;

    match &mut *skills {
        Some(registry) => registry.rescan().map_err(|e| e.to_string()),
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Load configuration from file
#[tauri::command]
async fn load_config(path: Option<String>, state: State<'_, AppState>) -> Result<String, String> {
    let config = Config::load_or_default(path.as_ref());
    let mut state_config = state.config.write().await;
    *state_config = config;
    Ok("Configuration loaded".to_string())
}

// ============================================================================
// Tauri Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::default();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_stats,
            init_skills,
            list_skills,
            get_skill,
            enable_skill,
            disable_skill,
            sync_skills,
            get_skills_config,
            refresh_skills,
            load_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
