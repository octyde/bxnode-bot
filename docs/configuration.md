# Configuration Reference

BXNode Bot uses a YAML configuration file to control all subsystems. Configuration can be loaded from:
- `config.yaml` in the current directory
- A path specified via `--config` / `-c` flag
- `BXNODE_CONFIG` environment variable

## Quick Start

```bash
# Copy the example configuration
cp config.example.yaml config.yaml

# Edit with your API keys and settings
vim config.yaml

# Start the server
bxnode-bot serve
```

## Configuration Sections

### Server

```yaml
server:
  host: "0.0.0.0"    # Bind address
  port: 3000         # HTTP port
  cors: true         # Enable CORS headers
```

### Agents

```yaml
agents:
  defaults:
    model: "anthropic/claude-3-opus"  # Default model for agents
    # image_model: "openai/gpt-4-vision"  # Optional vision model
```

### Providers

Configure LLM provider credentials and endpoints.

#### Anthropic (Claude)

```yaml
providers:
  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"  # Environment variable expansion
    # base_url: "https://api.anthropic.com"  # Optional custom endpoint
```

#### OpenAI (GPT)

```yaml
providers:
  openai:
    api_key: "${OPENAI_API_KEY}"
    # base_url: "https://api.openai.com"  # Optional custom endpoint
    # organization: "org-xxx"  # Optional organization ID
```

#### Ollama (Local)

```yaml
providers:
  ollama:
    base_url: "http://localhost:11434"  # Ollama server URL
```

### Channels

Configure messaging platform integrations. Each channel is optional.

#### Telegram

```yaml
channels:
  telegram:
    token: "YOUR_BOT_TOKEN"  # From @BotFather
    allowed_users: []        # Empty = allow all, or list of user IDs
    # allowed_chats: []      # Optional chat ID allowlist
```

#### Discord

```yaml
channels:
  discord:
    token: "YOUR_BOT_TOKEN"  # From Discord Developer Portal
    allowed_guilds: []       # Empty = allow all, or list of guild IDs
    # allowed_channels: []   # Optional channel ID allowlist
    # prefix: "!"            # Optional command prefix
    # allow_dms: true        # Respond to direct messages
```

#### Slack

```yaml
channels:
  slack:
    bot_token: "xoxb-YOUR-BOT-TOKEN"  # Bot OAuth token
    app_token: "xapp-YOUR-APP-TOKEN"  # App-level token for Socket Mode
    # allowed_channels: []   # Optional channel allowlist
    # allow_dms: true        # Respond to direct messages
    # mentions_only: false   # Only respond to @mentions
```

#### LINE

```yaml
channels:
  line:
    channel_access_token: "YOUR_CHANNEL_ACCESS_TOKEN"
    channel_secret: "YOUR_CHANNEL_SECRET"
    allowed_users: []  # Empty = allow all
```

#### Signal

```yaml
channels:
  signal:
    api_url: "http://localhost:8080"  # signal-cli-rest-api URL
    phone_number: "+1234567890"
    allowed_numbers: []  # Empty = allow all
```

#### Feishu

```yaml
channels:
  feishu:
    app_id: "YOUR_APP_ID"
    app_secret: "YOUR_APP_SECRET"
    verification_token: "YOUR_VERIFICATION_TOKEN"
    allowed_users: []  # Empty = allow all
```

### Cron

Configure scheduled job execution.

```yaml
cron:
  enabled: true                         # Enable/disable scheduler
  store_path: "~/.bxnode-bot/cron.json" # Persistent job store
  jobs: []                              # Jobs defined in config
```

#### Cron Job Format

```yaml
cron:
  jobs:
    - id: "daily-summary"
      schedule: "0 0 9 * * *"  # 6-field cron: sec min hour day month weekday
      payload:
        action: "generate-summary"
      description: "Daily morning summary"
      enabled: true
```

**Cron Schedule Format:** `sec min hour day month weekday`
- Seconds: 0-59
- Minutes: 0-59
- Hours: 0-23
- Day of month: 1-31
- Month: 1-12
- Day of week: 0-6 (Sunday = 0)

### Memory

Configure the long-term memory system for agents.

```yaml
memory:
  enabled: true                            # Enable/disable memory
  store_path: "~/.bxnode-bot/memory.jsonl" # JSONL storage file
  max_results: 5                           # Max search results
  ttl_days: 90                             # Memory expiry (0 = never)
```

### Plugins

Configure plugin loading (future feature).

```yaml
plugins:
  enabled: []      # List of enabled plugin IDs
  settings: {}     # Plugin-specific settings
```

## Environment Variable Expansion

Configuration values can reference environment variables using `${VAR_NAME}` syntax:

```yaml
providers:
  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"
```

This allows keeping secrets out of configuration files.

## Path Expansion

Paths starting with `~` are expanded to the user's home directory:

```yaml
memory:
  store_path: "~/.bxnode-bot/memory.jsonl"
```

## CLI Commands

### Configuration Management

```bash
# Show effective configuration
bxnode-bot config show

# Validate configuration file
bxnode-bot config validate --path config.yaml

# Initialize new configuration
bxnode-bot config init --output config.yaml
```

### Cron Management

```bash
# List cron jobs
bxnode-bot cron list

# Add a job
bxnode-bot cron add --id daily-check --schedule "0 0 9 * * *" --payload '{"action":"check"}'

# Remove a job
bxnode-bot cron remove daily-check

# Show recent runs
bxnode-bot cron runs --limit 10
```

### Memory Management

```bash
# List memories
bxnode-bot memory list --limit 20

# Search memories
bxnode-bot memory search "project requirements"

# Show statistics
bxnode-bot memory stats

# Get specific memory
bxnode-bot memory get <memory-id>

# Delete memory
bxnode-bot memory delete <memory-id>

# Compact store (remove deleted records)
bxnode-bot memory compact
```

## Default Values

| Setting | Default |
|---------|---------|
| `server.host` | `0.0.0.0` |
| `server.port` | `3000` |
| `server.cors` | `true` |
| `cron.enabled` | `true` |
| `cron.store_path` | `~/.bxnode-bot/cron.json` |
| `memory.enabled` | `true` |
| `memory.store_path` | `~/.bxnode-bot/memory.jsonl` |
| `memory.max_results` | `5` |
| `memory.ttl_days` | `90` |

## HTTP API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Simple health check (returns `{"status": "ok"}`) |
| `/health/detailed` | GET | Detailed status of all subsystems |
| `/stats` | GET | Metrics: provider/channel/job counts |
| `/v1/chat/completions` | POST | OpenAI-compatible chat API |
| `/v1/models` | GET | List available models |
| `/ws` | GET | WebSocket RPC endpoint |

## Feature Flags

Channels are feature-gated at compile time:

| Feature | Channel |
|---------|---------|
| `channel-telegram` | Telegram |
| `channel-discord` | Discord |
| `channel-slack` | Slack |
| `channel-line` | LINE |
| `channel-signal` | Signal |
| `channel-feishu` | Feishu |

Build with specific features:

```bash
# Build with specific channels
cargo build --release --features "channel-telegram,channel-discord"

# Build with all channels
cargo build --release --features full
```
