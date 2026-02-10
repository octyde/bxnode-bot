//! Skill CLI - Commands for managing agent skills

use std::path::PathBuf;

use crate::config::Config;
use crate::skills::loader::expand_path;
use crate::skills::{SkillLoader, SkillRegistry, SkillSyncer};

use super::SkillArgs;

/// Execute skill CLI commands
pub fn execute(args: SkillArgs) -> anyhow::Result<()> {
    let config = Config::load_or_default(args.config.as_ref());

    match args.action {
        super::SkillAction::List { all, verbose } => list_skills(&config, all, verbose),
        super::SkillAction::Info { name } => show_skill_info(&config, &name),
        super::SkillAction::Install { source, dir, force } => {
            install_skill(&config, &source, dir.as_deref(), force)
        }
        super::SkillAction::Update { name, force } => update_skills(&config, name.as_deref(), force),
        super::SkillAction::Enable { name } => enable_skill(&config, &name),
        super::SkillAction::Disable { name } => disable_skill(&config, &name),
        super::SkillAction::Sync { force } => sync_skills(&config, force),
        super::SkillAction::Search { query } => search_skills(&config, &query),
    }
}

/// List all discovered skills
fn list_skills(config: &Config, show_all: bool, verbose: bool) -> anyhow::Result<()> {
    if !config.skills.enabled {
        println!("Skills system is disabled in configuration.");
        return Ok(());
    }

    let registry = SkillRegistry::from_config(&config.skills)?;

    if registry.count() == 0 {
        println!("No skills found.");
        println!("\nSkill directories searched:");
        for dir in registry.directories() {
            println!("  - {}", dir.display());
        }
        println!("\nTo sync skills from the community registry, run:");
        println!("  bxnode-bot skill sync");
        return Ok(());
    }

    println!(
        "{:<25} {:<45} {:<10}",
        "NAME", "DESCRIPTION", "STATUS"
    );
    println!("{}", "-".repeat(80));

    let mut skills: Vec<_> = registry.list_info();
    skills.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));

    for skill in skills {
        let status = if skill.is_active { "active" } else { "inactive" };

        if !show_all && !skill.is_active {
            continue;
        }

        let desc = if skill.metadata.description.len() > 43 {
            format!("{}...", &skill.metadata.description[..40])
        } else {
            skill.metadata.description.clone()
        };

        println!("{:<25} {:<45} {:<10}", skill.metadata.name, desc, status);

        if verbose {
            if let Some(ref license) = skill.metadata.license {
                println!("  License: {}", license);
            }
            if let Some(ref compat) = skill.metadata.compatibility {
                println!("  Compatibility: {}", compat);
            }
            if !skill.metadata.allowed_tools.is_empty() {
                println!(
                    "  Allowed tools: {}",
                    skill.metadata.allowed_tools.join(", ")
                );
            }
            if skill.reference_count > 0 {
                println!("  References: {} file(s)", skill.reference_count);
            }
            if skill.script_count > 0 {
                println!("  Scripts: {} file(s)", skill.script_count);
            }
            println!("  Path: {}", skill.source_path);
            println!();
        }
    }

    let active = registry.active_count();
    let total = registry.count();
    println!("\n{} skill(s) active, {} total", active, total);

    Ok(())
}

/// Show detailed information about a specific skill
fn show_skill_info(config: &Config, name: &str) -> anyhow::Result<()> {
    if !config.skills.enabled {
        println!("Skills system is disabled in configuration.");
        return Ok(());
    }

    let mut registry = SkillRegistry::from_config(&config.skills)?;

    let skill = registry.get_mut(name).ok_or_else(|| {
        anyhow::anyhow!("Skill '{}' not found", name)
    })?;

    // Activate to load full content
    SkillLoader::activate_skill(skill)?;

    println!("Skill: {}", skill.metadata.name);
    println!("{}", "=".repeat(40));
    println!();
    println!("Description: {}", skill.metadata.description);
    println!();

    if let Some(ref license) = skill.metadata.license {
        println!("License: {}", license);
    }

    if let Some(ref compat) = skill.metadata.compatibility {
        println!("Compatibility: {}", compat);
    }

    if !skill.metadata.allowed_tools.is_empty() {
        println!(
            "Allowed Tools: {}",
            skill.metadata.allowed_tools.join(", ")
        );
    }

    if !skill.metadata.metadata.is_empty() {
        println!("\nMetadata:");
        for (key, value) in &skill.metadata.metadata {
            println!("  {}: {}", key, value);
        }
    }

    println!("\nPath: {}", skill.source_path.display());

    // List references
    let refs = SkillLoader::list_references(skill);
    if !refs.is_empty() {
        println!("\nReferences:");
        for r in refs {
            println!("  - {}", r);
        }
    }

    // List scripts
    let scripts = SkillLoader::list_scripts(skill);
    if !scripts.is_empty() {
        println!("\nScripts:");
        for s in scripts {
            println!("  - {}", s);
        }
    }

    // List assets
    let assets = SkillLoader::list_assets(skill);
    if !assets.is_empty() {
        println!("\nAssets:");
        for a in assets {
            println!("  - {}", a);
        }
    }

    // Show instructions preview
    if let Some(instructions) = skill.instructions() {
        println!("\nInstructions:");
        println!("{}", "-".repeat(40));
        // Show first 500 chars
        if instructions.len() > 500 {
            println!("{}...", &instructions[..500]);
            println!(
                "\n(Truncated. Full instructions: {} chars)",
                instructions.len()
            );
        } else {
            println!("{}", instructions);
        }
    }

    Ok(())
}

/// Install a skill from a GitHub URL or registry
fn install_skill(
    config: &Config,
    source: &str,
    target_dir: Option<&str>,
    force: bool,
) -> anyhow::Result<()> {
    let target = if let Some(dir) = target_dir {
        PathBuf::from(expand_path(dir))
    } else {
        // Use first configured directory as default
        let default_dir = config
            .skills
            .directories
            .first()
            .map(|d| expand_path(d))
            .unwrap_or_else(|| expand_path("~/.bxnode/skills"));
        PathBuf::from(default_dir)
    };

    // Ensure target directory exists
    std::fs::create_dir_all(&target)?;

    let syncer = SkillSyncer::new(target.clone());

    println!("Installing skill from: {}", source);

    let rt = tokio::runtime::Runtime::new()?;
    let skill = rt.block_on(syncer.install_from_url(source, force))?;

    println!("Installed skill: {}", skill.metadata.name);
    println!("  Description: {}", skill.metadata.description);
    println!("  Location: {}", skill.source_path.display());

    println!("\nTo enable this skill, add it to your config.yaml:");
    println!("  skills:");
    println!("    enabled_skills:");
    println!("      - {}", skill.metadata.name);

    Ok(())
}

/// Update installed skills
fn update_skills(config: &Config, name: Option<&str>, force: bool) -> anyhow::Result<()> {
    if !config.skills.enabled {
        println!("Skills system is disabled in configuration.");
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new()?;

    if let Some(skill_name) = name {
        // Update specific skill
        let registry = SkillRegistry::from_config(&config.skills)?;
        let skill = registry.get(skill_name).ok_or_else(|| {
            anyhow::anyhow!("Skill '{}' not found", skill_name)
        })?;

        println!("Updating skill: {}", skill_name);

        // Get the skill's source path parent directory
        let target_dir = skill
            .source_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid skill path"))?;

        let syncer = SkillSyncer::new(target_dir.to_path_buf());

        // Try to re-sync from awesome list
        let report = rt.block_on(syncer.sync_from_awesome_list(force))?;

        if report.synced.contains(&skill_name.to_string()) {
            println!("Updated skill: {}", skill_name);
        } else if report.skipped.contains(&skill_name.to_string()) {
            println!("Skill '{}' is already up to date (use --force to re-download)", skill_name);
        } else {
            println!("Could not find skill '{}' in the registry", skill_name);
        }
    } else {
        // Update all skills from configured sources
        for source in &config.skills.sync_sources {
            println!("Updating from: {}", source.repo);

            let target = PathBuf::from(expand_path(
                config
                    .skills
                    .directories
                    .first()
                    .unwrap_or(&"~/.bxnode/skills".to_string()),
            ));

            let syncer = SkillSyncer::new(target);
            let report = rt.block_on(syncer.sync_from_source(source, force))?;

            println!("  Synced: {}", report.synced.len());
            println!("  Skipped: {}", report.skipped.len());
            if !report.errors.is_empty() {
                println!("  Errors: {}", report.errors.len());
                for (name, err) in &report.errors {
                    println!("    - {}: {}", name, err);
                }
            }
        }
    }

    Ok(())
}

/// Enable a skill (show instructions)
fn enable_skill(config: &Config, name: &str) -> anyhow::Result<()> {
    if !config.skills.enabled {
        println!("Skills system is disabled in configuration.");
        return Ok(());
    }

    let registry = SkillRegistry::from_config(&config.skills)?;

    if registry.get(name).is_none() {
        println!("Skill '{}' not found.", name);
        println!("\nAvailable skills:");
        for skill in registry.list() {
            println!("  - {}", skill.metadata.name);
        }
        return Ok(());
    }

    if registry.is_active(name) {
        println!("Skill '{}' is already enabled.", name);
        return Ok(());
    }

    println!("To enable skill '{}', update your config.yaml:", name);
    println!();
    println!("  skills:");
    println!("    enabled_skills:");
    println!("      - {}", name);
    println!();
    println!("Or to enable all skills except specific ones:");
    println!();
    println!("  skills:");
    println!("    enabled_skills: []  # Empty = all skills");
    println!("    disabled_skills:");
    println!("      - some-other-skill");

    Ok(())
}

/// Disable a skill (show instructions)
fn disable_skill(config: &Config, name: &str) -> anyhow::Result<()> {
    if !config.skills.enabled {
        println!("Skills system is disabled in configuration.");
        return Ok(());
    }

    let registry = SkillRegistry::from_config(&config.skills)?;

    if registry.get(name).is_none() {
        println!("Skill '{}' not found.", name);
        return Ok(());
    }

    if !registry.is_active(name) {
        println!("Skill '{}' is already disabled.", name);
        return Ok(());
    }

    println!("To disable skill '{}', update your config.yaml:", name);
    println!();
    println!("  skills:");
    println!("    disabled_skills:");
    println!("      - {}", name);

    Ok(())
}

/// Sync skills from the awesome-openclaw-skills registry
fn sync_skills(config: &Config, force: bool) -> anyhow::Result<()> {
    let target = PathBuf::from(expand_path(
        config
            .skills
            .directories
            .first()
            .unwrap_or(&"~/.bxnode/skills".to_string()),
    ));

    // Ensure target directory exists
    std::fs::create_dir_all(&target)?;

    println!("Syncing skills to: {}", target.display());

    let syncer = SkillSyncer::new(target);

    let rt = tokio::runtime::Runtime::new()?;

    // First sync from configured sources
    if !config.skills.sync_sources.is_empty() {
        for source in &config.skills.sync_sources {
            println!("\nSyncing from configured source: {}", source.repo);
            let report = rt.block_on(syncer.sync_from_source(source, force))?;

            println!("  Synced: {} skill(s)", report.synced.len());
            for name in &report.synced {
                println!("    + {}", name);
            }

            if !report.skipped.is_empty() {
                println!("  Skipped: {} skill(s) (already exist)", report.skipped.len());
            }

            if !report.errors.is_empty() {
                println!("  Errors: {}", report.errors.len());
                for (name, err) in &report.errors {
                    println!("    - {}: {}", name, err);
                }
            }
        }
    } else {
        // Sync from awesome-openclaw-skills by default
        println!("\nSyncing from awesome-openclaw-skills...");
        println!("(This may take a while on first run)");

        let report = rt.block_on(syncer.sync_from_awesome_list(force))?;

        println!("\nSync complete!");
        println!("  Synced: {} skill(s)", report.synced.len());
        println!("  Skipped: {} skill(s) (already exist)", report.skipped.len());

        if !report.errors.is_empty() {
            println!("  Errors: {}", report.errors.len());
            for (name, err) in report.errors.iter().take(10) {
                println!("    - {}: {}", name, err);
            }
            if report.errors.len() > 10 {
                println!("    ... and {} more errors", report.errors.len() - 10);
            }
        }
    }

    println!("\nTo list available skills, run:");
    println!("  bxnode-bot skill list --all");

    Ok(())
}

/// Search for skills by query
fn search_skills(config: &Config, query: &str) -> anyhow::Result<()> {
    if !config.skills.enabled {
        println!("Skills system is disabled in configuration.");
        return Ok(());
    }

    let registry = SkillRegistry::from_config(&config.skills)?;

    let query_lower = query.to_lowercase();

    let matches: Vec<_> = registry
        .list()
        .into_iter()
        .filter(|skill| {
            skill.metadata.name.to_lowercase().contains(&query_lower)
                || skill
                    .metadata
                    .description
                    .to_lowercase()
                    .contains(&query_lower)
        })
        .collect();

    if matches.is_empty() {
        println!("No skills found matching '{}'", query);
        return Ok(());
    }

    println!(
        "Found {} skill(s) matching '{}':\n",
        matches.len(),
        query
    );

    println!("{:<25} {:<50}", "NAME", "DESCRIPTION");
    println!("{}", "-".repeat(75));

    for skill in matches {
        let desc = if skill.metadata.description.len() > 48 {
            format!("{}...", &skill.metadata.description[..45])
        } else {
            skill.metadata.description.clone()
        };

        let status = if registry.is_active(&skill.metadata.name) {
            " [active]"
        } else {
            ""
        };

        println!(
            "{:<25} {:<50}{}",
            skill.metadata.name, desc, status
        );
    }

    Ok(())
}
