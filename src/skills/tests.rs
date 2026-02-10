//! Unit tests for the skills module

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::loader::SkillLoader;
use super::registry::SkillRegistry;
use super::SkillMetadata;

/// Create a test skill directory with SKILL.md
fn create_test_skill(dir: &TempDir, name: &str, description: &str) -> PathBuf {
    let skill_dir = dir.path().join(name);
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_md = format!(
        r#"---
name: {}
description: {}
license: MIT
---

# {} Skill

This is a test skill.

## Usage

Use this skill when you need to test things.
"#,
        name, description, name
    );

    fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();
    skill_dir
}

/// Create a test skill with all optional fields (BXNode format)
fn create_full_test_skill(dir: &TempDir, name: &str) -> PathBuf {
    let skill_dir = dir.path().join(name);
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_md = format!(
        r#"---
name: {}
description: A fully featured test skill with all optional fields
license: Apache-2.0
compatibility: Requires git and docker
metadata:
  author: test-org
  version: "1.0.0"
allowed-tools:
  - Bash
  - Read
  - Write
---

# {} Skill

This is a comprehensive test skill.

## Instructions

1. First step
2. Second step
3. Third step

## Examples

### Example 1

Input: test
Output: result
"#,
        name, name
    );

    fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();

    // Create optional directories
    let refs_dir = skill_dir.join("references");
    fs::create_dir_all(&refs_dir).unwrap();
    fs::write(refs_dir.join("REFERENCE.md"), "# Reference\n\nDetailed docs...").unwrap();

    let scripts_dir = skill_dir.join("scripts");
    fs::create_dir_all(&scripts_dir).unwrap();
    fs::write(scripts_dir.join("helper.sh"), "#!/bin/bash\necho 'Hello'").unwrap();

    let assets_dir = skill_dir.join("assets");
    fs::create_dir_all(&assets_dir).unwrap();
    fs::write(assets_dir.join("template.txt"), "Template content").unwrap();

    skill_dir
}

/// Create a test skill using OpenClaw Agent Skills format
fn create_openclaw_skill(dir: &TempDir, name: &str) -> PathBuf {
    let skill_dir = dir.path().join(name);
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_md = format!(
        r#"---
name: {}
description: An OpenClaw-format skill with all standard fields
homepage: https://github.com/test/{}
user-invocable: true
disable-model-invocation: false
command-dispatch: tool
command-tool: Bash
---

# {} Skill

This is an OpenClaw-compatible skill.

## Instructions

Follow these steps to use this skill.
"#,
        name, name, name
    );

    fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();
    skill_dir
}

/// Create a utility skill that's not included in model prompts
fn create_utility_skill(dir: &TempDir, name: &str) -> PathBuf {
    let skill_dir = dir.path().join(name);
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_md = format!(
        r#"---
name: {}
description: A utility skill excluded from model invocation
user-invocable: true
disable-model-invocation: true
---

# {} Utility

This skill is only available via slash command.
"#,
        name, name
    );

    fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();
    skill_dir
}

#[test]
fn test_skill_metadata_validation() {
    // Valid metadata with all fields
    let metadata = SkillMetadata {
        name: "test-skill".to_string(),
        description: "A test skill".to_string(),
        homepage: None,
        user_invocable: true,
        disable_model_invocation: false,
        command_dispatch: None,
        command_tool: None,
        license: None,
        compatibility: None,
        metadata: Default::default(),
        allowed_tools: vec![],
    };
    assert!(metadata.validate().is_ok());

    // Invalid: empty name
    let metadata = SkillMetadata {
        name: "".to_string(),
        description: "A test skill".to_string(),
        homepage: None,
        user_invocable: true,
        disable_model_invocation: false,
        command_dispatch: None,
        command_tool: None,
        license: None,
        compatibility: None,
        metadata: Default::default(),
        allowed_tools: vec![],
    };
    assert!(metadata.validate().is_err());

    // Invalid: name starts with hyphen
    let metadata = SkillMetadata {
        name: "-test".to_string(),
        description: "A test skill".to_string(),
        homepage: None,
        user_invocable: true,
        disable_model_invocation: false,
        command_dispatch: None,
        command_tool: None,
        license: None,
        compatibility: None,
        metadata: Default::default(),
        allowed_tools: vec![],
    };
    assert!(metadata.validate().is_err());

    // Invalid: name ends with hyphen
    let metadata = SkillMetadata {
        name: "test-".to_string(),
        description: "A test skill".to_string(),
        homepage: None,
        user_invocable: true,
        disable_model_invocation: false,
        command_dispatch: None,
        command_tool: None,
        license: None,
        compatibility: None,
        metadata: Default::default(),
        allowed_tools: vec![],
    };
    assert!(metadata.validate().is_err());

    // Invalid: consecutive hyphens
    let metadata = SkillMetadata {
        name: "test--skill".to_string(),
        description: "A test skill".to_string(),
        homepage: None,
        user_invocable: true,
        disable_model_invocation: false,
        command_dispatch: None,
        command_tool: None,
        license: None,
        compatibility: None,
        metadata: Default::default(),
        allowed_tools: vec![],
    };
    assert!(metadata.validate().is_err());

    // Invalid: uppercase letters
    let metadata = SkillMetadata {
        name: "Test-Skill".to_string(),
        description: "A test skill".to_string(),
        homepage: None,
        user_invocable: true,
        disable_model_invocation: false,
        command_dispatch: None,
        command_tool: None,
        license: None,
        compatibility: None,
        metadata: Default::default(),
        allowed_tools: vec![],
    };
    assert!(metadata.validate().is_err());

    // Invalid: empty description
    let metadata = SkillMetadata {
        name: "test-skill".to_string(),
        description: "".to_string(),
        homepage: None,
        user_invocable: true,
        disable_model_invocation: false,
        command_dispatch: None,
        command_tool: None,
        license: None,
        compatibility: None,
        metadata: Default::default(),
        allowed_tools: vec![],
    };
    assert!(metadata.validate().is_err());
}

#[test]
fn test_parse_skill_metadata() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_test_skill(&dir, "my-skill", "A simple test skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    assert_eq!(skill_ref.metadata.name, "my-skill");
    assert_eq!(skill_ref.metadata.description, "A simple test skill");
    assert_eq!(skill_ref.metadata.license.as_deref(), Some("MIT"));
    assert!(!skill_ref.is_activated);
    assert!(skill_ref.instructions().is_none());
}

#[test]
fn test_parse_full_skill_metadata() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_full_test_skill(&dir, "full-skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    assert_eq!(skill_ref.metadata.name, "full-skill");
    assert_eq!(skill_ref.metadata.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(
        skill_ref.metadata.compatibility.as_deref(),
        Some("Requires git and docker")
    );
    assert_eq!(skill_ref.metadata.allowed_tools.len(), 3);
    assert!(skill_ref.metadata.allowed_tools.contains(&"Bash".to_string()));
    // Default OpenClaw fields
    assert!(skill_ref.metadata.user_invocable);
    assert!(!skill_ref.metadata.disable_model_invocation);
}

#[test]
fn test_parse_openclaw_skill() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_openclaw_skill(&dir, "openclaw-skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    assert_eq!(skill_ref.metadata.name, "openclaw-skill");
    assert_eq!(
        skill_ref.metadata.homepage.as_deref(),
        Some("https://github.com/test/openclaw-skill")
    );
    assert!(skill_ref.metadata.user_invocable);
    assert!(!skill_ref.metadata.disable_model_invocation);
    assert_eq!(skill_ref.metadata.command_dispatch.as_deref(), Some("tool"));
    assert_eq!(skill_ref.metadata.command_tool.as_deref(), Some("Bash"));
}

#[test]
fn test_parse_utility_skill() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_utility_skill(&dir, "utility-skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    assert_eq!(skill_ref.metadata.name, "utility-skill");
    assert!(skill_ref.metadata.user_invocable);
    assert!(skill_ref.metadata.disable_model_invocation);
}

#[test]
fn test_activate_skill() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_test_skill(&dir, "activatable", "A skill to activate");

    let mut skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    assert!(!skill_ref.is_activated);
    assert!(skill_ref.instructions().is_none());

    SkillLoader::activate_skill(&mut skill_ref).unwrap();

    assert!(skill_ref.is_activated);
    assert!(skill_ref.instructions().is_some());
    assert!(skill_ref.instructions().unwrap().contains("# activatable Skill"));
}

#[test]
fn test_scan_directory() {
    let dir = TempDir::new().unwrap();

    // Create multiple skills
    create_test_skill(&dir, "skill-a", "First skill");
    create_test_skill(&dir, "skill-b", "Second skill");
    create_test_skill(&dir, "skill-c", "Third skill");

    // Create a non-skill directory (no SKILL.md)
    fs::create_dir_all(dir.path().join("not-a-skill")).unwrap();

    let skills = SkillLoader::scan_directory(dir.path()).unwrap();

    assert_eq!(skills.len(), 3);

    let names: Vec<_> = skills.iter().map(|s| s.metadata.name.as_str()).collect();
    assert!(names.contains(&"skill-a"));
    assert!(names.contains(&"skill-b"));
    assert!(names.contains(&"skill-c"));
}

#[test]
fn test_skill_registry() {
    let dir = TempDir::new().unwrap();

    create_test_skill(&dir, "reg-skill-1", "Registry skill 1");
    create_test_skill(&dir, "reg-skill-2", "Registry skill 2");

    let mut registry = SkillRegistry::new();
    registry.add_directory(dir.path().to_path_buf());
    registry.scan_all().unwrap();

    assert_eq!(registry.count(), 2);
    assert_eq!(registry.active_count(), 0);

    // Enable a skill
    registry.enable("reg-skill-1").unwrap();
    assert!(registry.is_active("reg-skill-1"));
    assert!(!registry.is_active("reg-skill-2"));
    assert_eq!(registry.active_count(), 1);

    // Disable a skill
    registry.disable("reg-skill-1");
    assert!(!registry.is_active("reg-skill-1"));
    assert_eq!(registry.active_count(), 0);
}

#[test]
fn test_get_active_instructions() {
    let dir = TempDir::new().unwrap();

    create_test_skill(&dir, "active-1", "Active skill 1");
    create_test_skill(&dir, "active-2", "Active skill 2");
    create_test_skill(&dir, "inactive", "Inactive skill");

    let mut registry = SkillRegistry::new();
    registry.add_directory(dir.path().to_path_buf());
    registry.scan_all().unwrap();

    registry.enable("active-1").unwrap();
    registry.enable("active-2").unwrap();

    let instructions = registry.get_active_instructions();

    assert!(instructions.contains("## Skill: active-1"));
    assert!(instructions.contains("## Skill: active-2"));
    assert!(!instructions.contains("## Skill: inactive"));
}

#[test]
fn test_load_reference() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_full_test_skill(&dir, "ref-skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    let reference = SkillLoader::load_reference(&skill_ref, "REFERENCE.md").unwrap();
    assert!(reference.contains("# Reference"));
}

#[test]
fn test_load_script() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_full_test_skill(&dir, "script-skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    let script = SkillLoader::load_script(&skill_ref, "helper.sh").unwrap();
    assert!(script.contains("#!/bin/bash"));
}

#[test]
fn test_list_files() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_full_test_skill(&dir, "files-skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    let refs = SkillLoader::list_references(&skill_ref);
    assert_eq!(refs.len(), 1);
    assert!(refs.contains(&"REFERENCE.md".to_string()));

    let scripts = SkillLoader::list_scripts(&skill_ref);
    assert_eq!(scripts.len(), 1);
    assert!(scripts.contains(&"helper.sh".to_string()));

    let assets = SkillLoader::list_assets(&skill_ref);
    assert_eq!(assets.len(), 1);
    assert!(assets.contains(&"template.txt".to_string()));
}

#[test]
fn test_invalid_skill_md() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("invalid");
    fs::create_dir_all(&skill_dir).unwrap();

    // No frontmatter
    fs::write(skill_dir.join("SKILL.md"), "# No Frontmatter\n\nJust markdown").unwrap();

    let result = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md"));
    assert!(result.is_err());
}

#[test]
fn test_skill_ref_helpers() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_full_test_skill(&dir, "helper-skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    assert!(skill_ref.has_reference("REFERENCE.md"));
    assert!(!skill_ref.has_reference("nonexistent.md"));

    assert!(skill_ref.has_script("helper.sh"));
    assert!(!skill_ref.has_script("nonexistent.sh"));

    let asset_path = skill_ref.asset_path("template.txt");
    assert!(asset_path.exists());
}

#[test]
fn test_skill_info() {
    let dir = TempDir::new().unwrap();
    let skill_dir = create_full_test_skill(&dir, "info-skill");

    let skill_ref = SkillLoader::parse_skill_metadata(&skill_dir.join("SKILL.md")).unwrap();

    let info = super::SkillInfo::from(&skill_ref);

    assert_eq!(info.metadata.name, "info-skill");
    assert!(!info.is_active);
    assert!(!info.is_activated);
    assert_eq!(info.reference_count, 1);
    assert_eq!(info.script_count, 1);
}
