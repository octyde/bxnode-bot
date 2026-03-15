//! Tests for the project module

use super::*;
use tempfile::TempDir;

fn setup() -> (TempDir, TempDir, ProjectStore) {
    let meta_dir = TempDir::new().unwrap();
    let workspace_dir = TempDir::new().unwrap();
    let store = ProjectStore::new(
        meta_dir.path().to_path_buf(),
        workspace_dir.path().to_path_buf(),
    );
    (meta_dir, workspace_dir, store)
}

fn sample_project(store: &ProjectStore, name: &str) -> Project {
    Project {
        name: name.to_string(),
        workspace_dir: store.resolve_workspace_dir(name),
        description: Some("Test project".to_string()),
        model: None,
        system_prompt: None,
        coding_tools_enabled: true,
        shell_enabled: false,
        metadata: serde_json::json!({}),
        created_at: chrono::Utc::now(),
    }
}

#[test]
fn test_validate_project_name() {
    assert!(validate_project_name("myproject"));
    assert!(validate_project_name("my-project"));
    assert!(validate_project_name("my_project"));
    assert!(validate_project_name("project123"));
    assert!(!validate_project_name(""));
    assert!(!validate_project_name("."));
    assert!(!validate_project_name(".."));
    assert!(!validate_project_name("my project")); // spaces
    assert!(!validate_project_name("my/project")); // slashes
    assert!(!validate_project_name(&"a".repeat(65))); // too long
}

#[tokio::test]
async fn test_create_and_load() {
    let (_meta, _ws, store) = setup();
    let project = sample_project(&store, "test-project");

    store.create(&project).await.unwrap();

    let loaded = store.load("test-project").await.unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.name, "test-project");
    assert_eq!(loaded.description, Some("Test project".to_string()));
    assert!(loaded.coding_tools_enabled);
    assert!(!loaded.shell_enabled);
}

#[tokio::test]
async fn test_load_nonexistent() {
    let (_meta, _ws, store) = setup();
    let loaded = store.load("nonexistent").await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_create_duplicate() {
    let (_meta, _ws, store) = setup();
    let project = sample_project(&store, "dup");

    store.create(&project).await.unwrap();
    let result = store.create(&project).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update() {
    let (_meta, _ws, store) = setup();
    let mut project = sample_project(&store, "updatable");

    store.create(&project).await.unwrap();

    project.shell_enabled = true;
    project.model = Some("openai/gpt-4o".to_string());
    store.update(&project).await.unwrap();

    let loaded = store.load("updatable").await.unwrap().unwrap();
    assert!(loaded.shell_enabled);
    assert_eq!(loaded.model, Some("openai/gpt-4o".to_string()));
}

#[tokio::test]
async fn test_delete() {
    let (_meta, _ws, store) = setup();
    let project = sample_project(&store, "deletable");

    store.create(&project).await.unwrap();
    assert!(store.exists("deletable").await);

    store.delete("deletable").await.unwrap();
    assert!(!store.exists("deletable").await);
}

#[tokio::test]
async fn test_cannot_delete_default() {
    let (_meta, _ws, store) = setup();
    let result = store.delete("default").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list() {
    let (_meta, _ws, store) = setup();

    store.create(&sample_project(&store, "alpha")).await.unwrap();
    store.create(&sample_project(&store, "beta")).await.unwrap();
    store
        .create(&sample_project(&store, "default"))
        .await
        .unwrap();

    let projects = store.list().await.unwrap();
    assert_eq!(projects.len(), 3);
    // Default should be first
    assert_eq!(projects[0].name, "default");
}

#[tokio::test]
async fn test_resolve_workspace_dir() {
    let (_meta, _ws, store) = setup();
    let dir = store.resolve_workspace_dir("myproject");
    assert!(dir.ends_with("myproject"));
}

#[tokio::test]
async fn test_workspace_dir_created() {
    let (_meta, _ws, store) = setup();
    let project = sample_project(&store, "with-workspace");

    store.create(&project).await.unwrap();
    assert!(project.workspace_dir.exists());
}
