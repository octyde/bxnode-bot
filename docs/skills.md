# Agent Skills

BXNode Bot supports the [Agent Skills](https://agentskills.io/) format (also known as OpenClaw skills), an open standard for giving AI agents new capabilities and expertise through Markdown-based instructions.

## Overview

Skills are different from plugins:
- **Plugins** = Native Rust code that registers executable tools
- **Skills** = Markdown instructions that guide agent behavior (prompt engineering)

Both systems coexist and complement each other.

## Quick Start

### 1. Sync Skills from the Community Registry

```bash
# Download skills from awesome-openclaw-skills
bxnode-bot skill sync
```

### 2. List Available Skills

```bash
# List all skills
bxnode-bot skill list --all

# Search for specific skills
bxnode-bot skill search "debug"
```

### 3. Enable Skills in Configuration

```yaml
# config.yaml
skills:
  enabled: true
  enabled_skills:
    - debug-pro
    - code-review
```

### 4. Start the Server

```bash
bxnode-bot serve --config config.yaml
```

## Skill Directory Structure

Skills follow the [Agent Skills specification](https://agentskills.io/specification):

```
skill-name/
├── SKILL.md          # Required - YAML frontmatter + Markdown instructions
├── scripts/          # Optional - executable code
├── references/       # Optional - additional documentation
└── assets/           # Optional - templates, images, data
```

## SKILL.md Format

BXNode Bot supports the OpenClaw Agent Skills specification with BXNode extensions:

```yaml
---
name: skill-name              # Required, lowercase + hyphens (1-64 chars)
description: What it does     # Required, max 1024 chars

# OpenClaw standard fields:
homepage: https://example.com # Optional - skill website/documentation
user-invocable: true          # Optional - expose as /skill-name command (default: true)
disable-model-invocation: false # Optional - exclude from model prompt (default: false)
command-dispatch: tool        # Optional - bypass model, dispatch directly to tool
command-tool: Bash            # Optional - tool to invoke with command-dispatch

# BXNode extensions:
license: MIT                  # Optional - license identifier (e.g., MIT, Apache-2.0)
compatibility: ...            # Optional - environment requirements
allowed-tools: Bash Read      # Optional - pre-approved tools for this skill
metadata:                     # Optional - arbitrary key-value pairs
  author: your-name
  version: "1.0.0"
---

# Markdown instructions for agents

Step-by-step instructions, examples, edge cases...
```

### Field Reference

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | Yes | - | Unique skill identifier (lowercase, hyphens allowed) |
| `description` | string | Yes | - | Human-readable description (max 1024 chars) |
| `homepage` | URL | No | - | Link to skill documentation or repository |
| `user-invocable` | boolean | No | `true` | Whether skill is exposed as slash command |
| `disable-model-invocation` | boolean | No | `false` | Exclude skill from model's system prompt |
| `command-dispatch` | string | No | - | Set to "tool" to bypass model entirely |
| `command-tool` | string | No | - | Tool to invoke when command-dispatch is "tool" |
| `license` | string | No | - | SPDX license identifier |
| `compatibility` | string | No | - | Environment requirements (max 500 chars) |
| `allowed-tools` | list | No | `[]` | Pre-approved tools this skill can use |
| `metadata` | object | No | `{}` | Custom key-value metadata |

### OpenClaw Compatibility

Skills created for OpenClaw/Claude Code/Cursor work seamlessly with BXNode Bot:

```yaml
---
name: commit
description: Create conventional commits with AI assistance
homepage: https://github.com/author/commit-skill
user-invocable: true
---

# Commit Skill

Instructions for creating commits...
```

### Utility Skills (Model-Excluded)

Create skills that are only available via slash command, not included in the model's context:

```yaml
---
name: status-check
description: Quick system status utility
user-invocable: true
disable-model-invocation: true
---

# Status Check

This skill is only invoked via /status-check command.
```

### Direct Tool Dispatch

Bypass the model entirely and dispatch directly to a tool:

```yaml
---
name: quick-bash
description: Execute bash commands directly
command-dispatch: tool
command-tool: Bash
---

Arguments passed to /quick-bash are sent directly to Bash tool.
```

## CLI Commands

### List Skills

```bash
# List active skills
bxnode-bot skill list

# List all skills (including inactive)
bxnode-bot skill list --all

# Verbose output with details
bxnode-bot skill list --all --verbose
```

### Show Skill Info

```bash
# Show detailed information about a skill
bxnode-bot skill info debug-pro
```

### Install Skills

```bash
# Install from GitHub URL
bxnode-bot skill install https://github.com/openclaw/skills/tree/main/skills/author/skill-name

# Force reinstall
bxnode-bot skill install --force <url>

# Install to specific directory
bxnode-bot skill install --dir ./my-skills <url>
```

### Sync Skills

```bash
# Sync from configured sources (or awesome-openclaw-skills by default)
bxnode-bot skill sync

# Force re-download all skills
bxnode-bot skill sync --force
```

### Update Skills

```bash
# Update all skills
bxnode-bot skill update

# Update specific skill
bxnode-bot skill update debug-pro
```

### Search Skills

```bash
# Search by name or description
bxnode-bot skill search "code review"
```

### Enable/Disable Skills

```bash
# Get instructions for enabling a skill
bxnode-bot skill enable debug-pro

# Get instructions for disabling a skill
bxnode-bot skill disable debug-pro
```

Note: Enabling/disabling is done through configuration, not runtime commands.

## Configuration

```yaml
# config.yaml
skills:
  # Enable/disable the skills system
  enabled: true

  # Directories to scan for skills
  directories:
    - "~/.bxnode/skills"    # Global skills
    - "./skills"            # Workspace skills (higher priority)

  # Skills to explicitly enable (empty = all discovered skills)
  enabled_skills: []

  # Skills to explicitly disable
  disabled_skills: []

  # Sources to sync skills from
  sync_sources:
    - repo: "VoltAgent/awesome-openclaw-skills"
      branch: "main"
```

## Skill Storage Locations

Skills can be stored in two locations:

1. **Global**: `~/.bxnode/skills/` - Available to all projects
2. **Workspace**: `./skills/` - Project-specific skills (higher priority)

## Progressive Disclosure

Skills use progressive disclosure to minimize context usage:

1. **Level 1 (~100 tokens)**: Only `name` and `description` loaded at startup
2. **Level 2 (full instructions)**: Full `SKILL.md` body loaded when skill is activated
3. **Level 3 (on-demand)**: Reference files loaded only when needed

## Creating Custom Skills

### 1. Create Skill Directory

```bash
mkdir -p ~/.bxnode/skills/my-skill
```

### 2. Create SKILL.md

```markdown
---
name: my-skill
description: A custom skill that helps with specific tasks
homepage: https://github.com/yourname/my-skill
user-invocable: true
license: MIT
metadata:
  author: yourname
  version: "1.0.0"
---

# My Custom Skill

## When to Use

Use this skill when you need to...

## Instructions

1. First, do this...
2. Then, do that...

## Examples

### Example 1

...
```

### 3. Add References (Optional)

```bash
mkdir -p ~/.bxnode/skills/my-skill/references
echo "Detailed reference docs..." > ~/.bxnode/skills/my-skill/references/REFERENCE.md
```

### 4. Add Scripts (Optional)

```bash
mkdir -p ~/.bxnode/skills/my-skill/scripts
echo '#!/bin/bash\necho "Hello"' > ~/.bxnode/skills/my-skill/scripts/hello.sh
```

## Best Practices

1. **Keep instructions focused** - Each skill should have a single responsibility
2. **Use progressive disclosure** - Keep SKILL.md under 500 lines, use references for details
3. **Include examples** - Show typical inputs and outputs
4. **Document edge cases** - Help the agent handle unusual situations
5. **Version your skills** - Use the `metadata.version` field

## Troubleshooting

### Skills not being discovered

1. Check that `skills.enabled` is `true` in config
2. Verify skill directories exist and are readable
3. Ensure each skill folder contains a valid `SKILL.md`
4. Run `bxnode-bot skill list --all` to see all discovered skills

### Skill not loading

1. Validate SKILL.md has proper YAML frontmatter
2. Check that `name` and `description` fields are present
3. Ensure the directory name matches the skill name
4. Check logs for parsing errors

### Sync failing

1. Verify internet connectivity
2. Check GitHub API rate limits
3. Ensure target directory is writable
4. Try with `--force` to re-download

## Resources

- [Agent Skills Specification](https://agentskills.io/specification)
- [awesome-openclaw-skills](https://github.com/VoltAgent/awesome-openclaw-skills) - 1700+ community skills
- [Official Skills Repository](https://github.com/openclaw/skills)
