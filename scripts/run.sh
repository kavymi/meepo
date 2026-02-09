#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_DIR/target/release/meepo"
CONFIG_DIR="$HOME/.meepo"

# Build if binary doesn't exist or source is newer
if [ ! -f "$BINARY" ]; then
    echo "Binary not found. Building release..."
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
elif [ -n "$(find "$PROJECT_DIR/crates" -name '*.rs' -newer "$BINARY" 2>/dev/null | head -1)" ]; then
    echo "Source changed. Rebuilding..."
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
fi

# Initialize config if needed
if [ ! -d "$CONFIG_DIR" ]; then
    echo "First run — initializing config at $CONFIG_DIR..."
    "$BINARY" init
    echo ""
    echo "Edit $CONFIG_DIR/config.toml to enable channels, then re-run this script."
    exit 0
fi

# Check for API key
if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "Error: ANTHROPIC_API_KEY is not set."
    echo "  export ANTHROPIC_API_KEY=\"sk-ant-...\""
    exit 1
fi

# Pass all arguments through, default to "start" if none given
if [ $# -eq 0 ]; then
    echo "Starting Meepo..."
    exec "$BINARY" start
else
    exec "$BINARY" "$@"
fi
