#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-}"
FEATURES="full"

# Resolve config path: use src-tauri/config.yaml (Tauri app config) if it exists,
# otherwise fall back to config.yaml in project root
if [ -f "src-tauri/config.yaml" ]; then
  CONFIG="src-tauri/config.yaml"
elif [ -f "config.yaml" ]; then
  CONFIG="config.yaml"
elif [ -f "config.example.yaml" ]; then
  cp config.example.yaml config.yaml
  CONFIG="config.yaml"
  echo "Created config.yaml from config.example.yaml — edit it with your API keys."
else
  echo "Error: No config file found." >&2
  exit 1
fi

if [ "$MODE" = "desktop" ] || [ "$MODE" = "ui" ] || [ "$MODE" = "tauri" ]; then
  # Install UI dependencies if needed
  if [ ! -d "ui/node_modules" ]; then
    echo "Installing UI dependencies..."
    (cd ui && npm install)
  fi

  echo "Starting Tauri desktop app..."
  shift
  exec cargo tauri dev "$@"
else
  BINARY="target/release/bxnode-bot"

  # Check if rebuild is needed: Rust source or UI source changed
  NEED_BUILD=false
  if [ ! -f "$BINARY" ]; then
    NEED_BUILD=true
  elif [ -n "$(find src Cargo.toml -newer "$BINARY" 2>/dev/null | head -1)" ]; then
    NEED_BUILD=true
  elif [ -n "$(find ui/src -newer "$BINARY" 2>/dev/null | head -1)" ]; then
    NEED_BUILD=true
  fi

  if [ "$NEED_BUILD" = true ]; then
    # Rebuild UI if sources are newer than dist
    if [ ! -d "ui/dist" ] || [ -n "$(find ui/src -newer ui/dist/index.html 2>/dev/null | head -1)" ]; then
      echo "Building UI..."
      (cd ui && npm run build)
    fi
    echo "Building bxnode-bot (--features $FEATURES)..."
    cargo build --release --features "$FEATURES"
  fi

  echo "Starting bxnode-bot with config: $CONFIG"
  exec "$BINARY" serve --config "$CONFIG" "$@"
fi
