#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_DIR/target/release/meepo"
CONFIG_DIR="$HOME/.meepo"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
DIM='\033[2m'
BOLD='\033[1m'
NC='\033[0m'

# Build if binary doesn't exist or source is newer
if [ ! -f "$BINARY" ]; then
    echo -e "${DIM}Binary not found — building release...${NC}"
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
    echo ""
elif [ -n "$(find "$PROJECT_DIR/crates" -name '*.rs' -newer "$BINARY" 2>/dev/null | head -1)" ]; then
    echo -e "${DIM}Source changed — rebuilding...${NC}"
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
    echo ""
fi

# Initialize config if needed
if [ ! -d "$CONFIG_DIR" ]; then
    echo -e "${YELLOW}First run detected.${NC}"
    echo ""
    echo "  Run the setup script for guided configuration:"
    echo -e "  ${BOLD}$SCRIPT_DIR/setup.sh${NC}"
    echo ""
    echo "  Or initialize a bare config with:"
    echo -e "  ${DIM}$BINARY init${NC}"
    exit 0
fi

# Check for at least one LLM provider key (or Ollama running locally)
HAS_PROVIDER=false
if [ -n "${ANTHROPIC_API_KEY:-}" ]; then HAS_PROVIDER=true; fi
if [ -n "${OPENAI_API_KEY:-}" ]; then HAS_PROVIDER=true; fi
if [ -n "${GOOGLE_AI_API_KEY:-}" ]; then HAS_PROVIDER=true; fi
if curl -sf http://localhost:11434/api/tags >/dev/null 2>&1; then HAS_PROVIDER=true; fi

if [ "$HAS_PROVIDER" = false ]; then
    echo -e "${RED}No LLM provider configured.${NC}"
    echo ""
    echo "  Set one of these:"
    echo -e "  ${DIM}export ANTHROPIC_API_KEY=\"sk-ant-...\"${NC}"
    echo -e "  ${DIM}export OPENAI_API_KEY=\"sk-...\"${NC}"
    echo -e "  ${DIM}export GOOGLE_AI_API_KEY=\"AIza...\"${NC}"
    echo -e "  ${DIM}# Or start Ollama: ollama serve${NC}"
    echo ""
    echo "  Or run setup:"
    echo -e "  ${DIM}$SCRIPT_DIR/setup.sh${NC}"
    exit 1
fi

# Pass all arguments through, default to "start" if none given
if [ $# -eq 0 ]; then
    echo -e "${GREEN}Starting Meepo...${NC}"
    exec "$BINARY" start
else
    exec "$BINARY" "$@"
fi
