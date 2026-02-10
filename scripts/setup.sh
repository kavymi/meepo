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
TOTAL_STEPS=7
SHELL_RC=""

# ── Colors ───────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

# ── Helpers ──────────────────────────────────────────────────────

print_step() {
    local step="$1"
    local title="$2"
    echo ""
    echo -e "${BLUE}${BOLD}[$step/$TOTAL_STEPS] $title${NC}"
    echo -e "${DIM}$(printf '%.0s─' {1..50})${NC}"
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

print_dim() {
    echo -e "  ${DIM}$1${NC}"
}

print_url() {
    echo -e "  ${CYAN}→${NC} ${BOLD}$1${NC}"
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

# Open a URL in the default browser (macOS)
open_url() {
    local url="$1"
    if [[ "$(uname)" == "Darwin" ]] && command -v open &>/dev/null; then
        open "$url" 2>/dev/null
        return 0
    fi
    return 1
}

# Detect shell config file
detect_shell_rc() {
    if [ -n "$SHELL_RC" ]; then return; fi
    if [ -f "$HOME/.zshrc" ]; then
        SHELL_RC="$HOME/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then
        SHELL_RC="$HOME/.bashrc"
    elif [ -f "$HOME/.bash_profile" ]; then
        SHELL_RC="$HOME/.bash_profile"
    fi
}

# Save an env var to shell config if not already there
save_env_var() {
    local var_name="$1"
    local var_value="$2"
    local comment="${3:-}"
    detect_shell_rc
    if [ -n "$SHELL_RC" ] && ! grep -q "$var_name" "$SHELL_RC"; then
        echo "" >> "$SHELL_RC"
        [ -n "$comment" ] && echo "# $comment" >> "$SHELL_RC"
        echo "export $var_name=\"$var_value\"" >> "$SHELL_RC"
        print_dim "Saved to $SHELL_RC"
    fi
    export "$var_name=$var_value"
}

# Prompt for an API key with browser-open and clipboard support.
# Usage: capture_key "DISPLAY_NAME" "URL" "ENV_VAR" "PREFIX" "COMMENT"
# Sets the captured value in the variable named by ENV_VAR.
capture_key() {
    local name="$1"
    local url="$2"
    local env_var="$3"
    local prefix="${4:-}"
    local comment="${5:-Meepo}"

    # Already set?
    local current="${!env_var:-}"
    if [ -n "$current" ]; then
        local masked="${current:0:7}...${current: -4}"
        print_ok "$env_var already set ($masked)"
        return 0
    fi

    echo ""
    print_url "$url"
    echo ""

    if ask_yn "Open this in your browser?"; then
        open_url "$url" && print_dim "Opened in browser — switch over and grab the key"
        echo ""
    fi

    echo -e "  ${DIM}Paste the key below, or copy it and press Enter to read from clipboard.${NC}"
    read -rsp "  $name key: " key_input
    echo ""

    # If empty input, try clipboard
    if [ -z "$key_input" ] && [[ "$(uname)" == "Darwin" ]]; then
        key_input="$(pbpaste 2>/dev/null || true)"
        if [ -n "$key_input" ]; then
            print_dim "Read from clipboard"
        fi
    fi

    if [ -z "$key_input" ]; then
        print_warn "No key entered — set $env_var later"
        echo -e "  ${DIM}export $env_var=\"...\""
        return 1
    fi

    # Validate prefix if provided
    if [ -n "$prefix" ] && [[ "$key_input" != "$prefix"* ]]; then
        print_warn "Key doesn't start with '$prefix' — may not be valid, saving anyway"
    else
        print_ok "Key captured"
    fi

    save_env_var "$env_var" "$key_input" "$comment - $name"
    return 0
}

# ── Welcome ──────────────────────────────────────────────────────

clear
echo ""
echo -e "${BOLD}"
echo "    ╔═══════════════════════════════════╗"
echo "    ║         meepo  setup              ║"
echo "    ║     local ai agent for macOS      ║"
echo "    ╚═══════════════════════════════════╝"
echo -e "${NC}"

# ── Step 1: Prerequisites ───────────────────────────────────────

print_step 1 "Prerequisites"

# Rust
if command -v cargo &>/dev/null; then
    print_ok "Rust $(rustc --version 2>/dev/null | cut -d' ' -f2)"
else
    print_err "Rust not found"
    echo ""
    echo -e "  Install: ${BOLD}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
    exit 1
fi

# macOS
if [[ "$(uname)" != "Darwin" ]]; then
    print_warn "Not macOS — iMessage and AppleScript tools won't work"
else
    print_ok "macOS $(sw_vers -productVersion 2>/dev/null || echo '')"
fi

# Optional tools
command -v gh &>/dev/null && print_ok "GitHub CLI" || print_dim "gh not found (optional — brew install gh)"
command -v claude &>/dev/null && print_ok "Claude CLI" || print_dim "claude not found (optional — npm i -g @anthropic-ai/claude-code)"

# ── Step 2: Build ───────────────────────────────────────────────

print_step 2 "Build"

BINARY_PATH="$REPO_DIR/target/release/meepo"

if [ -f "$BINARY_PATH" ]; then
    print_ok "Binary exists at $BINARY_PATH"
    if ask_yn "Rebuild?"; then
<<<<<<< Updated upstream
        echo -e "  ${DIM}Building...${NC}"
        (cd "$REPO_DIR" && cargo build --release 2>&1 | tail -1)
        print_ok "Build complete"
    fi
else
    echo -e "  ${DIM}First build — this takes ~2 minutes...${NC}"
    (cd "$REPO_DIR" && cargo build --release 2>&1 | tail -1)
    print_ok "Built $BINARY_PATH"
=======
        echo "  Building (this takes ~2 minutes)..."
        echo ""
        (cd "$REPO_DIR" && cargo build --release)
        echo ""
        print_ok "Build complete"
    fi
else
    echo "  Building (this takes ~2 minutes on first build)..."
    echo ""
    (cd "$REPO_DIR" && cargo build --release)
    echo ""
    print_ok "Build complete: $BINARY_PATH"
>>>>>>> Stashed changes
fi

# ── Step 3: Config ──────────────────────────────────────────────

print_step 3 "Configuration"

if [ -f "$CONFIG_FILE" ]; then
    print_ok "Config exists at $CONFIG_FILE"
    if ask_yn "Overwrite with fresh defaults?"; then
        rm -f "$CONFIG_FILE"
        "$BINARY_PATH" init 2>/dev/null
        print_ok "Config re-initialized"
    fi
else
    "$BINARY_PATH" init 2>/dev/null
    print_ok "Created $CONFIG_DIR/ with config.toml, SOUL.md, MEMORY.md"
fi

# ── Step 4: Anthropic API Key ───────────────────────────────────

print_step 4 "Anthropic API Key ${DIM}(required)${NC}"

echo "  Powers all of Meepo's thinking via Claude."
capture_key "Anthropic" \
    "https://console.anthropic.com/settings/keys" \
    "ANTHROPIC_API_KEY" \
    "sk-ant-" \
    "Meepo"

# ── Step 5: Tavily API Key ──────────────────────────────────────

print_step 5 "Tavily API Key ${DIM}(optional — web search)${NC}"

echo "  Enables the web_search tool and cleaner URL extraction."
echo "  Free tier available — no credit card needed."

if ask_yn "Set up Tavily?"; then
    capture_key "Tavily" \
        "https://app.tavily.com/home" \
        "TAVILY_API_KEY" \
        "tvly-" \
        "Meepo - Tavily web search"
else
    print_dim "Skipped — web_search tool won't be available"
fi

# ── Step 6: Channels ────────────────────────────────────────────

print_step 6 "Channels"

echo "  Meepo can listen on Discord, Slack, and/or iMessage."
echo -e "  ${DIM}You can also skip all channels and use 'meepo ask' from the CLI.${NC}"
echo ""

# ── Discord ──

if ask_yn "Enable Discord?"; then
    echo ""
    echo -e "  ${BOLD}Quick setup:${NC} Create a bot, copy its token, invite it to your server."
    print_url "https://discord.com/developers/applications"

    if ask_yn "Open Discord Developer Portal?"; then
        open_url "https://discord.com/developers/applications"
        print_dim "Opened in browser"
    fi

    echo ""
    echo -e "  ${DIM}In the portal:${NC}"
    echo -e "  ${DIM}  1. New Application → \"Meepo\" → Bot → Reset Token → copy it${NC}"
    echo -e "  ${DIM}  2. Turn on MESSAGE CONTENT INTENT${NC}"
    echo -e "  ${DIM}  3. OAuth2 → URL Generator → scope: bot → Send Messages${NC}"
    echo -e "  ${DIM}  4. Open the generated URL to invite bot to your server${NC}"
    echo ""

    echo -e "  ${DIM}Paste the bot token below, or copy it and press Enter for clipboard.${NC}"
    read -rsp "  Bot token: " DISCORD_TOKEN
    echo ""

    if [ -z "$DISCORD_TOKEN" ] && [[ "$(uname)" == "Darwin" ]]; then
        DISCORD_TOKEN="$(pbpaste 2>/dev/null || true)"
        [ -n "$DISCORD_TOKEN" ] && print_dim "Read from clipboard"
    fi

    if [ -n "$DISCORD_TOKEN" ]; then
        save_env_var "DISCORD_BOT_TOKEN" "$DISCORD_TOKEN" "Meepo - Discord bot"
        print_ok "Token saved"
    else
        print_warn "No token — set DISCORD_BOT_TOKEN later"
    fi

    echo ""
    echo -e "  ${DIM}To get your user ID: enable Developer Mode in Discord settings,${NC}"
    echo -e "  ${DIM}then right-click your name → Copy User ID.${NC}"
    DISCORD_USER=$(ask_value "Your Discord user ID (or Enter to skip)" "")

    # Update config
    sed -i '' '/^\[channels\.discord\]$/,/^\[/{s/^enabled = false/enabled = true/;}' "$CONFIG_FILE"
    if [ -n "$DISCORD_USER" ]; then
        sed -i '' "/^\[channels\.discord\]$/,/^\[/{s/^allowed_users = \[\]/allowed_users = [\"$DISCORD_USER\"]/;}" "$CONFIG_FILE"
    fi

    print_ok "Discord enabled"
fi

# ── Slack ──

echo ""
if ask_yn "Enable Slack?"; then
    echo ""
    echo -e "  ${BOLD}Quick setup:${NC} Create a Slack app, add scopes, install to workspace."
    print_url "https://api.slack.com/apps"

    if ask_yn "Open Slack API portal?"; then
        open_url "https://api.slack.com/apps"
        print_dim "Opened in browser"
    fi

    echo ""
    echo -e "  ${DIM}In the portal:${NC}"
    echo -e "  ${DIM}  1. Create New App → From scratch → \"Meepo\"${NC}"
    echo -e "  ${DIM}  2. OAuth & Permissions → add scopes:${NC}"
    echo -e "  ${DIM}     chat:write, channels:read, im:history, im:read, users:read${NC}"
    echo -e "  ${DIM}  3. Install to Workspace → copy Bot User OAuth Token${NC}"
    echo ""

    echo -e "  ${DIM}Paste the token below, or copy it and press Enter for clipboard.${NC}"
    read -rsp "  Bot token (xoxb-...): " SLACK_TOKEN
    echo ""

    if [ -z "$SLACK_TOKEN" ] && [[ "$(uname)" == "Darwin" ]]; then
        SLACK_TOKEN="$(pbpaste 2>/dev/null || true)"
        [ -n "$SLACK_TOKEN" ] && print_dim "Read from clipboard"
    fi

    if [ -n "$SLACK_TOKEN" ]; then
        save_env_var "SLACK_BOT_TOKEN" "$SLACK_TOKEN" "Meepo - Slack bot"
        print_ok "Token saved"
    else
        print_warn "No token — set SLACK_BOT_TOKEN later"
    fi

    sed -i '' '/^\[channels\.slack\]$/,/^\[/{s/^enabled = false/enabled = true/;}' "$CONFIG_FILE"
    print_ok "Slack enabled"
fi

# ── iMessage ──

echo ""
if ask_yn "Enable iMessage?"; then
    echo ""
    echo -e "  ${BOLD}Requires:${NC} Full Disk Access for your terminal app."
    print_url "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"

<<<<<<< Updated upstream
    if ask_yn "Open System Settings → Full Disk Access?"; then
        open_url "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
        print_dim "Opened System Settings — toggle ON for your terminal"
    fi

    echo ""
    CONTACTS=$(ask_value "Allowed contacts (comma-separated phones/emails, or Enter for all)" "")
    PREFIX=$(ask_value "Trigger prefix" "/d")
=======
    CONTACTS=$(ask_value "Allowed contacts (comma-separated phones/emails)" "")
>>>>>>> Stashed changes

    sed -i '' '/^\[channels\.imessage\]$/,/^\[/{s/^enabled = false/enabled = true/;}' "$CONFIG_FILE"

    if [ -n "$CONTACTS" ]; then
        TOML_CONTACTS=$(echo "$CONTACTS" | sed 's/[[:space:]]*,[[:space:]]*/", "/g' | sed 's/^/["/' | sed 's/$/"]/')
        sed -i '' "/^\[channels\.imessage\]$/,/^\[/{s|^allowed_contacts = \[\]|allowed_contacts = $TOML_CONTACTS|;}" "$CONFIG_FILE"
    fi

<<<<<<< Updated upstream
    if [ "$PREFIX" != "/d" ]; then
        sed -i '' "/^\[channels\.imessage\]$/,/^\[/{s|^trigger_prefix = \"/d\"|trigger_prefix = \"$PREFIX\"|;}" "$CONFIG_FILE"
    fi

    print_ok "iMessage enabled"
=======
    print_ok "iMessage enabled in config"
>>>>>>> Stashed changes
fi

# ── Step 7: Verify ──────────────────────────────────────────────

print_step 7 "Verify"

if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
    echo -e "  ${DIM}Testing API connection...${NC}"
    if "$BINARY_PATH" ask "Say 'hello' in one word." 2>/dev/null | head -5; then
        echo ""
        print_ok "API connection works"
    else
        print_warn "API test failed — check your key"
    fi
else
    print_warn "No API key set — skipping connection test"
fi

# ── Summary ──────────────────────────────────────────────────────

echo ""
echo -e "${BLUE}${BOLD}═══ Setup Complete ═══${NC}"
echo ""

# Config summary
echo -e "  ${BOLD}Files${NC}"
echo -e "  ${DIM}Config  ${NC} $CONFIG_FILE"
echo -e "  ${DIM}Binary  ${NC} $BINARY_PATH"
echo -e "  ${DIM}Soul    ${NC} $CONFIG_DIR/workspace/SOUL.md"
echo -e "  ${DIM}Memory  ${NC} $CONFIG_DIR/workspace/MEMORY.md"
echo ""

# API key status
echo -e "  ${BOLD}Keys${NC}"
for var in ANTHROPIC_API_KEY TAVILY_API_KEY DISCORD_BOT_TOKEN SLACK_BOT_TOKEN; do
    if [ -n "${!var:-}" ]; then
        echo -e "  ${GREEN}✓${NC} $var"
    else
        echo -e "  ${DIM}·${NC} ${DIM}$var (not set)${NC}"
    fi
done
echo ""

# Next steps
echo -e "  ${BOLD}Next steps${NC}"
echo -e "  ${CYAN}\$${NC} meepo start              ${DIM}# start the daemon${NC}"
echo -e "  ${CYAN}\$${NC} meepo ask \"Hello\"        ${DIM}# one-shot question${NC}"
echo -e "  ${CYAN}\$${NC} scripts/install.sh       ${DIM}# run on login${NC}"
echo ""

if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo -e "  ${YELLOW}${BOLD}Don't forget:${NC} export ANTHROPIC_API_KEY=\"sk-ant-...\""
    echo ""
fi

detect_shell_rc
if [ -n "$SHELL_RC" ]; then
    print_dim "Keys were saved to $SHELL_RC — run 'source $SHELL_RC' or open a new terminal."
fi

# Suggest adding binary to PATH
if ! command -v meepo &>/dev/null; then
    echo ""
    print_dim "Tip: ln -sf $BINARY_PATH /usr/local/bin/meepo"
fi

echo ""
