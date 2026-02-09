#!/bin/bash
set -euo pipefail

# ╔══════════════════════════════════════════════════════════════════╗
# ║                     Meepo Setup Script                          ║
# ║                                                                 ║
# ║  Interactive first-time setup. Builds the binary, initializes   ║
# ║  config, walks through API keys, and enables channels.          ║
# ╚══════════════════════════════════════════════════════════════════╝

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG_DIR="$HOME/.meepo"
CONFIG_FILE="$CONFIG_DIR/config.toml"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m' # No Color

print_header() {
    echo ""
    echo -e "${BLUE}${BOLD}═══ $1 ═══${NC}"
    echo ""
}

print_ok() {
    echo -e "  ${GREEN}✓${NC} $1"
}

print_warn() {
    echo -e "  ${YELLOW}!${NC} $1"
}

print_err() {
    echo -e "  ${RED}✗${NC} $1"
}

ask_yn() {
    local prompt="$1"
    local default="${2:-n}"
    local yn
    if [ "$default" = "y" ]; then
        read -rp "  $prompt [Y/n]: " yn
        yn="${yn:-y}"
    else
        read -rp "  $prompt [y/N]: " yn
        yn="${yn:-n}"
    fi
    [[ "$yn" =~ ^[Yy] ]]
}

ask_value() {
    local prompt="$1"
    local default="${2:-}"
    local value
    if [ -n "$default" ]; then
        read -rp "  $prompt [$default]: " value
        echo "${value:-$default}"
    else
        read -rp "  $prompt: " value
        echo "$value"
    fi
}

# ── Welcome ───────────────────────────────────────────────────────

echo ""
echo -e "${BOLD}  ╔═══════════════════════════════════╗${NC}"
echo -e "${BOLD}  ║         Meepo Setup               ║${NC}"
echo -e "${BOLD}  ║   Local AI Agent for macOS         ║${NC}"
echo -e "${BOLD}  ╚═══════════════════════════════════╝${NC}"
echo ""

# ── Step 1: Check Prerequisites ───────────────────────────────────

print_header "Step 1: Checking Prerequisites"

# Check Rust
if command -v cargo &>/dev/null; then
    RUST_VER=$(rustc --version 2>/dev/null | cut -d' ' -f2)
    print_ok "Rust toolchain found (rustc $RUST_VER)"
else
    print_err "Rust not found. Install it:"
    echo "      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check macOS
if [[ "$(uname)" != "Darwin" ]]; then
    print_warn "Not running macOS — some features (iMessage, AppleScript tools) won't work"
else
    print_ok "macOS detected"
fi

# Check optional tools
if command -v gh &>/dev/null; then
    print_ok "GitHub CLI (gh) found"
else
    print_warn "GitHub CLI not found — code tools won't work. Install: brew install gh"
fi

if command -v claude &>/dev/null; then
    print_ok "Claude CLI found"
else
    print_warn "Claude CLI not found — write_code tool won't work. Install: npm install -g @anthropic-ai/claude-code"
fi

# ── Step 2: Build ─────────────────────────────────────────────────

print_header "Step 2: Building Meepo"

BINARY_PATH="$REPO_DIR/target/release/meepo"

if [ -f "$BINARY_PATH" ]; then
    print_ok "Binary already exists at $BINARY_PATH"
    if ask_yn "Rebuild?"; then
        echo "  Building (this takes ~2 minutes)..."
        (cd "$REPO_DIR" && cargo build --release 2>&1 | tail -1)
        print_ok "Build complete"
    fi
else
    echo "  Building (this takes ~2 minutes on first build)..."
    (cd "$REPO_DIR" && cargo build --release 2>&1 | tail -1)
    print_ok "Build complete: $BINARY_PATH"
fi

# ── Step 3: Initialize Config ─────────────────────────────────────

print_header "Step 3: Initializing Configuration"

if [ -f "$CONFIG_FILE" ]; then
    print_ok "Config already exists at $CONFIG_FILE"
    if ask_yn "Overwrite with fresh config?"; then
        "$BINARY_PATH" init 2>/dev/null
        print_ok "Config re-initialized"
    fi
else
    "$BINARY_PATH" init 2>/dev/null
    print_ok "Created $CONFIG_DIR/"
    print_ok "Created config.toml, SOUL.md, MEMORY.md"
fi

# ── Step 4: Anthropic API Key ─────────────────────────────────────

print_header "Step 4: Anthropic API Key (Required)"

echo "  Meepo needs an Anthropic API key to talk to Claude."
echo ""
echo "  Get yours at: ${BOLD}https://console.anthropic.com/settings/keys${NC}"
echo "    1. Sign up / log in"
echo "    2. Go to Settings → API Keys"
echo "    3. Click \"Create Key\""
echo "    4. Copy the key (starts with sk-ant-)"
echo ""

CURRENT_KEY="${ANTHROPIC_API_KEY:-}"
if [ -n "$CURRENT_KEY" ]; then
    MASKED="${CURRENT_KEY:0:7}...${CURRENT_KEY: -4}"
    print_ok "ANTHROPIC_API_KEY already set in environment ($MASKED)"
else
    print_warn "ANTHROPIC_API_KEY not set in environment"
    echo ""

    if ask_yn "Enter your API key now?"; then
        read -rsp "  Paste your API key (hidden): " API_KEY
        echo ""

        if [[ "$API_KEY" == sk-ant-* ]]; then
            print_ok "Key looks valid"

            # Detect shell config file
            SHELL_RC=""
            if [ -f "$HOME/.zshrc" ]; then
                SHELL_RC="$HOME/.zshrc"
            elif [ -f "$HOME/.bashrc" ]; then
                SHELL_RC="$HOME/.bashrc"
            elif [ -f "$HOME/.bash_profile" ]; then
                SHELL_RC="$HOME/.bash_profile"
            fi

            if [ -n "$SHELL_RC" ] && ask_yn "Add to $SHELL_RC?"; then
                echo "" >> "$SHELL_RC"
                echo "# Meepo - Anthropic API key" >> "$SHELL_RC"
                echo "export ANTHROPIC_API_KEY=\"$API_KEY\"" >> "$SHELL_RC"
                print_ok "Added to $SHELL_RC"
                print_warn "Run 'source $SHELL_RC' or open a new terminal for it to take effect"
                export ANTHROPIC_API_KEY="$API_KEY"
            else
                echo ""
                echo "  Add this to your shell config manually:"
                echo "    export ANTHROPIC_API_KEY=\"$API_KEY\""
                export ANTHROPIC_API_KEY="$API_KEY"
            fi
        else
            print_warn "Key doesn't start with 'sk-ant-' — may not be valid"
            export ANTHROPIC_API_KEY="$API_KEY"
        fi
    else
        echo ""
        echo "  Set it later:"
        echo "    export ANTHROPIC_API_KEY=\"sk-ant-...\""
    fi
fi

# ── Step 5: Tavily API Key (Optional) ────────────────────────────

print_header "Step 5: Tavily API Key (Optional)"

echo "  Tavily enables web search and clean URL content extraction."
echo "  Without it, Meepo still works — you just won't have the web_search tool."
echo ""

CURRENT_TAVILY="${TAVILY_API_KEY:-}"
if [ -n "$CURRENT_TAVILY" ]; then
    MASKED_T="${CURRENT_TAVILY:0:5}...${CURRENT_TAVILY: -4}"
    print_ok "TAVILY_API_KEY already set in environment ($MASKED_T)"
else
    if ask_yn "Set up Tavily web search?"; then
        echo ""
        echo "  Get your key at: ${BOLD}https://tavily.com${NC}"
        echo "    1. Sign up at tavily.com"
        echo "    2. Copy your API key from the dashboard (starts with tvly-)"
        echo ""

        read -rsp "  Paste your Tavily API key (hidden): " TAVILY_KEY
        echo ""

        if [ -n "$TAVILY_KEY" ]; then
            SHELL_RC="${SHELL_RC:-$HOME/.zshrc}"
            if [ -f "$SHELL_RC" ] && ! grep -q "TAVILY_API_KEY" "$SHELL_RC"; then
                echo "" >> "$SHELL_RC"
                echo "# Meepo - Tavily API key (web search)" >> "$SHELL_RC"
                echo "export TAVILY_API_KEY=\"$TAVILY_KEY\"" >> "$SHELL_RC"
                print_ok "Added TAVILY_API_KEY to $SHELL_RC"
            fi
            export TAVILY_API_KEY="$TAVILY_KEY"
        fi
    else
        print_warn "Skipping — web_search tool won't be available"
    fi
fi

# ── Step 6: Enable Channels ───────────────────────────────────────

print_header "Step 6: Enable Channels"

echo "  Meepo can listen on Discord, Slack, and/or iMessage."
echo "  You can also skip all channels and just use 'meepo ask' from the CLI."
echo ""

# ── Discord ──

if ask_yn "Enable Discord?"; then
    echo ""
    echo "  Discord bot setup:"
    echo "    1. Go to ${BOLD}https://discord.com/developers/applications${NC}"
    echo "    2. New Application → name it \"Meepo\" → go to Bot"
    echo "    3. Reset Token → copy it"
    echo "    4. Enable: MESSAGE CONTENT INTENT (under Privileged Gateway Intents)"
    echo "    5. OAuth2 → URL Generator → scopes: bot → permissions: Send Messages"
    echo "    6. Copy the URL → open it → invite to your server"
    echo "    7. Right-click your name in Discord → Copy User ID"
    echo ""

    DISCORD_TOKEN=$(ask_value "Bot token (or press Enter to set later)")
    DISCORD_USER=$(ask_value "Your Discord user ID (18-digit number)")

    if [ -n "$DISCORD_TOKEN" ]; then
        # Write token to shell config
        SHELL_RC="${SHELL_RC:-$HOME/.zshrc}"
        if [ -f "$SHELL_RC" ] && ! grep -q "DISCORD_BOT_TOKEN" "$SHELL_RC"; then
            echo "export DISCORD_BOT_TOKEN=\"$DISCORD_TOKEN\"" >> "$SHELL_RC"
            print_ok "Added DISCORD_BOT_TOKEN to $SHELL_RC"
        fi
    fi

    # Update config.toml
    sed -i '' 's/^\[channels\.discord\]$/[channels.discord]/' "$CONFIG_FILE"
    sed -i '' '/^\[channels\.discord\]$/,/^\[/{s/^enabled = false/enabled = true/;}' "$CONFIG_FILE"
    if [ -n "$DISCORD_USER" ]; then
        sed -i '' "/^\[channels\.discord\]$/,/^\[/{s/^allowed_users = \[\]/allowed_users = [\"$DISCORD_USER\"]/;}" "$CONFIG_FILE"
    fi

    print_ok "Discord enabled in config"
fi

# ── Slack ──

echo ""
if ask_yn "Enable Slack?"; then
    echo ""
    echo "  Slack bot setup:"
    echo "    1. Go to ${BOLD}https://api.slack.com/apps${NC}"
    echo "    2. Create New App → From scratch → name it \"Meepo\""
    echo "    3. OAuth & Permissions → add Bot Token Scopes:"
    echo "       chat:write, channels:read, im:history, im:read, users:read"
    echo "    4. Install to Workspace → authorize"
    echo "    5. Copy the Bot User OAuth Token (starts with xoxb-)"
    echo ""

    SLACK_TOKEN=$(ask_value "Bot token (or press Enter to set later)")

    if [ -n "$SLACK_TOKEN" ]; then
        SHELL_RC="${SHELL_RC:-$HOME/.zshrc}"
        if [ -f "$SHELL_RC" ] && ! grep -q "SLACK_BOT_TOKEN" "$SHELL_RC"; then
            echo "export SLACK_BOT_TOKEN=\"$SLACK_TOKEN\"" >> "$SHELL_RC"
            print_ok "Added SLACK_BOT_TOKEN to $SHELL_RC"
        fi
    fi

    sed -i '' '/^\[channels\.slack\]$/,/^\[/{s/^enabled = false/enabled = true/;}' "$CONFIG_FILE"
    print_ok "Slack enabled in config"
fi

# ── iMessage ──

echo ""
if ask_yn "Enable iMessage?"; then
    echo ""
    echo "  iMessage requires Full Disk Access for your terminal:"
    echo "    System Settings → Privacy & Security → Full Disk Access"
    echo "    → Toggle ON for Terminal (or iTerm, Alacritty, etc.)"
    echo ""

    CONTACTS=$(ask_value "Allowed contacts (comma-separated phones/emails)" "")
    PREFIX=$(ask_value "Trigger prefix" "/d")

    sed -i '' '/^\[channels\.imessage\]$/,/^\[/{s/^enabled = false/enabled = true/;}' "$CONFIG_FILE"

    if [ -n "$CONTACTS" ]; then
        # Convert comma-separated to TOML array
        TOML_CONTACTS=$(echo "$CONTACTS" | sed 's/[[:space:]]*,[[:space:]]*/", "/g' | sed 's/^/["/' | sed 's/$/"]/')
        sed -i '' "/^\[channels\.imessage\]$/,/^\[/{s|^allowed_contacts = \[\]|allowed_contacts = $TOML_CONTACTS|;}" "$CONFIG_FILE"
    fi

    if [ "$PREFIX" != "/d" ]; then
        sed -i '' "/^\[channels\.imessage\]$/,/^\[/{s|^trigger_prefix = \"/d\"|trigger_prefix = \"$PREFIX\"|;}" "$CONFIG_FILE"
    fi

    print_ok "iMessage enabled in config"
fi

# ── Step 7: Verify ────────────────────────────────────────────────

print_header "Step 7: Verification"

# Quick test
if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
    echo "  Testing API connection..."
    if "$BINARY_PATH" ask "Say 'hello' in one word." 2>/dev/null | head -5; then
        echo ""
        print_ok "API connection works!"
    else
        print_warn "API test failed — check your API key"
    fi
else
    print_warn "Skipping API test (no API key set yet)"
fi

# ── Done ──────────────────────────────────────────────────────────

print_header "Setup Complete"

echo "  Config:  $CONFIG_FILE"
echo "  Binary:  $BINARY_PATH"
echo "  Soul:    $CONFIG_DIR/workspace/SOUL.md  (edit to customize personality)"
echo "  Memory:  $CONFIG_DIR/workspace/MEMORY.md (auto-updated)"
echo ""
echo -e "  ${BOLD}Quick start:${NC}"
echo "    meepo start              # Start the daemon"
echo "    meepo ask \"Hello\"        # One-shot question"
echo "    meepo --debug start      # Start with debug logs"
echo ""
echo -e "  ${BOLD}Run as background service:${NC}"
echo "    scripts/install.sh       # Install as macOS launch agent"
echo "    scripts/uninstall.sh     # Remove launch agent"
echo ""

if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo -e "  ${YELLOW}${BOLD}IMPORTANT:${NC} Set your API key before running:"
    echo "    export ANTHROPIC_API_KEY=\"sk-ant-...\""
    echo ""
fi

# Suggest adding binary to PATH
if ! command -v meepo &>/dev/null; then
    echo "  Tip: Add the binary to your PATH:"
    echo "    ln -sf $BINARY_PATH /usr/local/bin/meepo"
    echo "    # or"
    echo "    alias meepo='$BINARY_PATH'"
    echo ""
fi
