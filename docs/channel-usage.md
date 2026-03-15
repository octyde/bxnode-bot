# BXNode Bot - Channel Usage Guide

This document covers how to use BXNode Bot through messaging channels such as Telegram, Discord, Slack, and others. It describes configuration, commands, the project routing protocol, coding tools, and the WebSocket RPC interface.

---

## Table of Contents

- [Overview](#overview)
- [Supported Channels](#supported-channels)
- [Getting Started (Telegram)](#getting-started-telegram)
- [Commands Reference](#commands-reference)
- [Project Routing (@project)](#project-routing-project)
- [Coding Tools](#coding-tools)
- [Approval System](#approval-system)
- [Streaming Responses](#streaming-responses)
- [Message Chunking](#message-chunking)
- [Server Management](#server-management)
- [WebSocket RPC Reference](#websocket-rpc-reference)

---

## Overview

Channels are messaging platform integrations that let you interact with BXNode Bot from Telegram, Discord, Slack, LINE, Signal, or Feishu. Messages sent to the bot are processed by an LLM agent and responses are sent back through the same channel.

Key features:
- **Slash commands** — local commands like `/help`, `/status`, `/projects` are handled directly without calling the LLM
- **Project routing** — use `@projectname message` to route prompts to specific coding projects
- **Coding tools** — the agent can read, write, edit, and search files in project workspaces
- **Streaming** — responses stream in real-time on channels that support message editing (Telegram, Discord)
- **Approval system** — require admin approval before new users can interact with the bot
- **Sessions** — conversation history is organized into sessions with full transcript persistence

---

## Supported Channels

### Telegram

```yaml
channels:
  telegram:
    token: "YOUR_BOT_TOKEN"        # From @BotFather
    allowed_users: []               # User IDs to allow (empty = allow all)
    approval_required: false        # Require admin approval for new users
```

Features: text, images, audio, video, files, stickers, location, message editing (streaming), bot command menu registration.

### Discord

```yaml
channels:
  discord:
    token: "YOUR_BOT_TOKEN"
    allowed_guilds: []              # Guild IDs to allow (empty = allow all)
```

Features: text, images, audio, video, files, message editing (streaming), DM support.

### Slack

```yaml
channels:
  slack:
    bot_token: "xoxb-YOUR-BOT-TOKEN"
    app_token: "xapp-YOUR-APP-TOKEN"
```

Features: text, basic media, Socket Mode for real-time events.

### LINE

```yaml
channels:
  line:
    channel_access_token: "YOUR_CHANNEL_ACCESS_TOKEN"
    channel_secret: "YOUR_CHANNEL_SECRET"
    allowed_users: []               # Empty = allow all
```

Features: webhook-based integration, user allowlisting.

### Signal

```yaml
channels:
  signal:
    api_url: "http://localhost:8080"  # signal-cli-rest-api URL
    phone_number: "+1234567890"
    allowed_numbers: []               # Empty = allow all
```

Features: requires [signal-cli-rest-api](https://github.com/bbernhard/signal-cli-rest-api) service, phone number allowlisting.

### Feishu

```yaml
channels:
  feishu:
    app_id: "YOUR_APP_ID"
    app_secret: "YOUR_APP_SECRET"
    verification_token: "YOUR_VERIFICATION_TOKEN"
    allowed_users: []               # Empty = allow all
```

Features: Chinese enterprise messaging platform, user allowlisting.

---

## Getting Started (Telegram)

1. Create a bot via [@BotFather](https://t.me/BotFather) and copy the token.

2. Add the token to your config file (`config.yaml` or `src-tauri/config.yaml`):
   ```yaml
   channels:
     telegram:
       token: "8360482836:AAHsOQhfSwK..."
       allowed_users: []
   ```

3. Start the server:
   ```bash
   ./start.sh
   # or from the desktop app: click "Start Server" on the Channels page
   ```

4. Send `/help` to your bot in Telegram to verify it's working.

The bot automatically registers all commands with Telegram's command menu on startup, so typing `/` will show the available commands.

---

## Commands Reference

All commands are processed locally and never sent to the LLM. In Telegram groups, the `@botname` suffix (e.g., `/help@mybot`) is automatically stripped.

### General

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/commands` | Alias for `/help` |
| `/status` | Show bot status (model, providers, context usage, token stats, skills, sessions) |
| `/model` | Show current LLM model |
| `/ping` | Check if bot is alive (responds "Pong!") |
| `/id` | Show Chat ID, User ID, and Channel name |
| `/version` | Show bot version |

### Sessions

| Command | Description |
|---------|-------------|
| `/sessions` | List all sessions for the current chat, sorted by most recent |
| `/newsession` | Start a fresh session (clears in-memory context, old sessions are preserved) |

### Projects

| Command | Arguments | Description |
|---------|-----------|-------------|
| `/projects` | | List all projects with session counts, workspace paths, and active marker |
| `/project` | `<name>` | Switch to a project (creates a new session in that project if needed) |
| `/newproject` | `<name> [--shell] [--model provider/model]` | Create a new project |
| `/deleteproject` | `<name>` | Delete a project and all associated sessions |
| `/projectinfo` | `[name]` | Show project details (workspace, model, tools, sessions, system prompt) |
| `/projectconfig` | `<key> <value>` | Update project settings |

#### `/newproject` examples

```
/newproject myapp
/newproject backend --shell
/newproject frontend --model anthropic/claude-sonnet-4-20250514
/newproject api --shell --model openai/gpt-4o
```

The `--shell` flag enables shell command execution in the project workspace. The `--model` flag sets a per-project LLM model override.

#### `/projectconfig` keys

| Key | Values | Description |
|-----|--------|-------------|
| `model` | `provider/model-name` | Set LLM model override for the project |
| `shell` | `on` / `off` | Enable/disable shell command execution |
| `coding` | `on` / `off` | Enable/disable coding tools (file read/write/edit) |
| `systemprompt` | `<text>` or `clear` | Set a custom system prompt, or `clear` to remove it |

Examples:
```
/projectconfig shell on
/projectconfig model anthropic/claude-sonnet-4-20250514
/projectconfig systemprompt You are a Python expert. Always use type hints.
/projectconfig systemprompt clear
```

### Skills

| Command | Description |
|---------|-------------|
| `/skills` | List all active skills with name, status, and description |
| `/<skill-name>` | Invoke a specific skill (if active) |

---

## Project Routing (@project)

Use the `@projectname` prefix to route messages to a specific coding project:

```
@myapp What files are in this project?
@myapp Create a hello.py that prints hello world
@backend Show me the database schema
```

### How it works

1. The bot parses `@projectname <message>` from the beginning of your message
2. Validates that the project exists (suggests `/newproject` if not)
3. Uses a **project-specific context** — separate from your default conversation, allowing multiple project contexts in the same chat
4. Loads the project's coding tools, model override, and system prompt
5. Prefixes the response with the project name: `myproject: Here's the result...`

### Context isolation

Each `@project` mention uses its own conversation context keyed as `channel:chat_id:project`. This means:
- `@frontend` and `@backend` maintain separate conversation histories
- Messages without `@project` use the default context (`channel:chat_id`)
- Each project context has its own session and transcript

### Project name rules

Project names must contain only lowercase letters, numbers, hyphens, and underscores, and be 1-64 characters long.

---

## Coding Tools

When a project has `coding_tools_enabled: true` (the default), the LLM agent has access to 8 workspace-scoped tools. All file paths are sandboxed to the project's workspace directory.

| Tool | Description | Parameters |
|------|-------------|------------|
| `file_read` | Read file contents, supports line ranges | `path` (required), `line_start`, `line_end` |
| `file_write` | Write or create files | `path`, `content` (required), `create_dirs` (default: true) |
| `file_edit` | Search/replace edit within a file | `path`, `old_text`, `new_text` (all required) |
| `file_delete` | Delete a file | `path` (required) |
| `list_directory` | List directory tree with depth limit | `path` (default: "."), `depth` (default: 3) |
| `file_search` | Regex search within files (grep-like) | `pattern` (required), `path`, `file_pattern` |
| `shell_exec` | Run shell commands (requires `shell_enabled`) | `command` (required), `timeout` (default: 30s) |
| `git_status` | Show git status and recent log | None |

### Path sandboxing

All paths are resolved relative to the project workspace and validated to prevent directory traversal. Paths like `../../etc/passwd` are rejected. The `blocked_paths` config setting (e.g., `~/.ssh/*`) provides additional protection.

### Shell execution

`shell_exec` is disabled by default. Enable it per-project:
```
/projectconfig shell on
```
or when creating a project:
```
/newproject myapp --shell
```

Shell commands execute in the project's workspace directory with a 30-second timeout.

---

## Approval System

The approval system gates access to the bot, requiring admin approval before new users can interact.

### Configuration

Enable per-channel in the config:
```yaml
channels:
  telegram:
    token: "..."
    approval_required: true
```

### Flow

1. **New user sends a message** — automatically added to the pending queue. Receives: *"Welcome! Your access request has been submitted for admin approval."*
2. **Pending user sends another message** — receives: *"Your access request is still pending admin approval. Please wait."*
3. **Admin approves/rejects** — via the desktop app's Approval Panel, or via WebSocket RPC (`approvals.approve` / `approvals.reject`)
4. **Approved user** — messages are processed normally from now on
5. **Rejected user** — messages are silently ignored

Approval state persists in `~/.bxnode-bot/approvals.json`.

---

## Streaming Responses

On channels that support message editing (Telegram, Discord), responses stream in real-time:

1. Bot sends an initial "thinking" indicator (`▌`)
2. As the LLM generates text, the message is edited in-place with accumulated content
3. On completion, the final message replaces the placeholder
4. If the response exceeds the message length limit, additional chunks are sent as separate messages

Channels without edit support (Slack, LINE, Signal) receive the complete response as a single message after generation finishes.

---

## Message Chunking

Telegram enforces a 4,096-character message limit. BXNode Bot automatically splits long responses:

- Messages are split at **line boundaries** (not mid-line) with a 4,000-character threshold
- The first chunk is delivered as the reply (or edit of the streaming placeholder)
- Remaining chunks are sent as separate follow-up messages
- Multi-byte characters are handled safely (no mid-character splits)

---

## Server Management

### From the Desktop App

The Channels page provides server controls:

| Action | Description |
|--------|-------------|
| **Start Server** | Launch the bot server (visible when stopped) |
| **Stop** | Gracefully shut down the server |
| **Restart** | Stop and re-launch the server process (works for both local and attached servers) |
| **Attach** | Connect to an already-running BXNode Bot server by port |
| **Detach** | Disconnect from an attached server without stopping it |

### From the Command Line

```bash
# Start the server
./start.sh

# Start with Tauri desktop app
./start.sh desktop

# Direct binary usage
./target/release/bxnode-bot serve --config config.yaml --port 18500
```

### Server Restart Protocol

When restarted via the desktop app or WebSocket RPC:
1. Graceful shutdown signal is sent
2. All channels are stopped
3. A new server process is spawned with the same executable and arguments
4. The desktop app polls until the new server is reachable, then reconnects

---

## WebSocket RPC Reference

Connect to `ws://localhost:{port}/ws` for real-time events and RPC calls.

### Request Format

```json
{
  "id": "unique-request-id",
  "method": "method.name",
  "params": { ... }
}
```

### Response Format

```json
{
  "id": "unique-request-id",
  "result": { ... }
}
```

Or on error:

```json
{
  "id": "unique-request-id",
  "error": "Error description"
}
```

### Methods

#### Server

| Method | Params | Returns |
|--------|--------|---------|
| `health` | None | `{"status": "ok", "version": "0.1.0"}` |
| `server.restart` | None | `{"restarting": true}` |

#### Models & Providers

| Method | Params | Returns |
|--------|--------|---------|
| `models` | None | `{"models": [{"id", "provider", "name", "context_length"}]}` |
| `providers` | None | `{"providers": ["anthropic", "openai", ...]}` |

#### Channels

| Method | Params | Returns |
|--------|--------|---------|
| `channels.status` | None | `{"channels": [{"id", "name", "connected", "error"}]}` |

#### Sessions

| Method | Params | Returns |
|--------|--------|---------|
| `sessions.list` | `channel?`, `chat_id?` | `{"sessions": [{"id", "channel", "chat_id", "project", "title", "created_at", "updated_at"}]}` |
| `sessions.transcript` | `session_id` | `{"entries": [...]}` |

#### Projects

| Method | Params | Returns |
|--------|--------|---------|
| `projects.list` | None | `{"projects": [{"name", "workspace_dir", "model", "coding_tools_enabled", "shell_enabled", "created_at", "session_count"}]}` |
| `projects.get` | `name` | `{"name", "workspace_dir", "description", "model", "system_prompt", "coding_tools_enabled", "shell_enabled", "created_at", "session_count"}` |
| `projects.create` | `name`, `description?`, `model?`, `system_prompt?`, `coding_tools_enabled?`, `shell_enabled?` | `{"name", "workspace_dir"}` |
| `projects.update` | `name`, `model?`, `system_prompt?`, `description?`, `shell_enabled?`, `coding_tools_enabled?` | `{"success": true}` |
| `projects.delete` | `name` | `{"success": true}` (cannot delete "default") |

#### Approvals

| Method | Params | Returns |
|--------|--------|---------|
| `approvals.list` | None | `{"approvals": [{"id", "user_id", "user_name", "channel", "chat_id", "first_message", "timestamp"}]}` |
| `approvals.approve` | `id` | `{"success": true}` |
| `approvals.reject` | `id` | `{"success": true}` |

### Broadcast Events

The WebSocket also pushes real-time events (no request needed):

| Event | Fields |
|-------|--------|
| `message.incoming` | `id`, `channel`, `chat_id`, `user_id`, `user_name`, `content`, `timestamp`, `session_id` |
| `message.outgoing` | `channel`, `chat_id`, `content`, `reply_to`, `timestamp`, `session_id` |
| `message.error` | `channel`, `chat_id`, `error`, `timestamp`, `session_id` |
| `approval.request` | `id`, `user_id`, `user_name`, `channel`, `chat_id`, `first_message`, `timestamp` |
| `approval.resolved` | `id`, `user_id`, `action` (`"approved"` or `"rejected"`) |

---

## Workspace Configuration

Global defaults for coding projects are configured in `config.yaml`:

```yaml
workspace:
  base_dir: "~/projects"            # Default workspace base directory
  coding_tools_enabled: true        # Enable coding tools for new projects
  shell_enabled: false              # Enable shell execution for new projects
  blocked_paths:                    # Paths to never access (glob patterns)
    - "~/.ssh/*"
    - "~/.gnupg/*"
```

Each project's workspace is created at `{base_dir}/{project_name}/`. Project-level settings override these defaults and can be changed via `/projectconfig`.
