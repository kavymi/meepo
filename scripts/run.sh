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

# Check for API key
if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo -e "${RED}ANTHROPIC_API_KEY is not set.${NC}"
    echo ""
    echo "  Set it now:"
    echo -e "  ${DIM}export ANTHROPIC_API_KEY=\"sk-ant-...\"${NC}"
    echo ""
    echo "  Or run setup again:"
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
