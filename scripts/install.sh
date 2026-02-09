#!/bin/bash
set -euo pipefail

PLIST_NAME="com.meepo.meepo"
PLIST_PATH="$HOME/Library/LaunchAgents/$PLIST_NAME.plist"
# Look for the binary in common locations
if [ -f "$HOME/.cargo/bin/meepo" ]; then
    BINARY_PATH="$HOME/.cargo/bin/meepo"
elif [ -f "$(dirname "$0")/../target/release/meepo" ]; then
    BINARY_PATH="$(cd "$(dirname "$0")/.." && pwd)/target/release/meepo"
elif command -v meepo &>/dev/null; then
    BINARY_PATH="$(command -v meepo)"
else
    BINARY_PATH=""
fi
LOG_DIR="$HOME/.meepo/logs"

echo "Installing Meepo as a macOS launch agent..."

# Check if binary was found
if [ -z "$BINARY_PATH" ] || [ ! -f "$BINARY_PATH" ]; then
    echo "Error: Meepo binary not found."
    echo ""
    echo "Build it first with one of:"
    echo "  cargo build --release              # Binary at target/release/meepo"
    echo "  cargo install --path crates/meepo-cli  # Binary at ~/.cargo/bin/meepo"
    exit 1
fi

echo "Using binary: $BINARY_PATH"

# Create log directory
mkdir -p "$LOG_DIR"

# Build environment variable entries for the plist.
# launchd doesn't inherit shell env vars, so we snapshot them at install time.
ENV_ENTRIES="        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/opt/homebrew/bin:$HOME/.cargo/bin</string>"

for VAR_NAME in ANTHROPIC_API_KEY TAVILY_API_KEY DISCORD_BOT_TOKEN SLACK_BOT_TOKEN; do
    VAR_VALUE="${!VAR_NAME:-}"
    if [ -n "$VAR_VALUE" ]; then
        ENV_ENTRIES="$ENV_ENTRIES
        <key>$VAR_NAME</key>
        <string>$VAR_VALUE</string>"
    fi
done

# Create launchd plist
cat > "$PLIST_PATH" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$PLIST_NAME</string>
    <key>ProgramArguments</key>
    <array>
        <string>$BINARY_PATH</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$LOG_DIR/meepo.out.log</string>
    <key>StandardErrorPath</key>
    <string>$LOG_DIR/meepo.err.log</string>
    <key>EnvironmentVariables</key>
    <dict>
$ENV_ENTRIES
    </dict>
</dict>
</plist>
EOF

echo "Created launchd plist at $PLIST_PATH"

# Show which env vars were captured
echo ""
echo "Environment variables captured in plist:"
for VAR_NAME in ANTHROPIC_API_KEY TAVILY_API_KEY DISCORD_BOT_TOKEN SLACK_BOT_TOKEN; do
    if [ -n "${!VAR_NAME:-}" ]; then
        echo "  ✓ $VAR_NAME"
    else
        echo "  - $VAR_NAME (not set)"
    fi
done
echo ""
echo "Note: If you add or change API keys later, re-run this script to update the plist."

# Unload first if already loaded (idempotent reinstall)
launchctl unload "$PLIST_PATH" 2>/dev/null || true

# Load the agent
launchctl load "$PLIST_PATH"
echo ""
echo "Meepo started and will run on login."
echo ""
echo "Commands:"
echo "  launchctl stop $PLIST_NAME     # Stop"
echo "  launchctl start $PLIST_NAME    # Start"
echo "  launchctl unload $PLIST_PATH   # Uninstall"
echo "  tail -f $LOG_DIR/meepo.out.log # View logs"
