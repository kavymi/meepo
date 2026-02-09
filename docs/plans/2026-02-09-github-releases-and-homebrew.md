# GitHub Releases + curl Installer + Homebrew Tap Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let users install Meepo with one command — `curl -sSL https://raw.githubusercontent.com/kavymi/meepo/main/install.sh | bash` or `brew install kavymi/tap/meepo` — instead of cloning and building from source.

**Architecture:** GitHub Actions builds release binaries for 3 targets (macOS ARM64, macOS x86_64, Windows x86_64) on every version tag push. A universal install script detects OS/arch and downloads the right binary. A Homebrew formula wraps the same release artifacts for `brew install`.

**Tech Stack:** GitHub Actions, Bash, PowerShell, Homebrew formula DSL

---

### Task 1: Add `meepo setup` CLI subcommand

The interactive setup wizard currently lives in `scripts/setup.sh` — a standalone script that only works if you cloned the repo. We need a `meepo setup` command built into the binary so users who installed via curl/brew can run the setup wizard without the repo.

**Files:**
- Modify: `crates/meepo-cli/src/main.rs`

**Step 1: Add Setup subcommand to CLI**

Add to the `Commands` enum in `main.rs:34`:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Start the Meepo daemon
    Start,
    /// Stop a running Meepo daemon
    Stop,
    /// Send a one-shot message to the agent
    Ask {
        /// The message to send
        message: String,
    },
    /// Initialize config directory and default config
    Init,
    /// Interactive first-time setup wizard
    Setup,
    /// Show current configuration
    Config,
}
```

Add the match arm in `main()`:

```rust
Commands::Setup => cmd_setup().await,
```

**Step 2: Implement cmd_setup()**

This is an embedded version of setup.sh's core flow — init config, prompt for API key, test connection. It doesn't need channel setup (that's in config.toml). Add after `cmd_init()`:

```rust
async fn cmd_setup() -> Result<()> {
    use std::io::{self, Write, BufRead};

    println!("\n  Meepo Setup\n  ───────────\n");

    // Step 1: Init config
    cmd_init().await?;
    let config_dir = config::config_dir();
    let config_path = config_dir.join("config.toml");

    // Step 2: Anthropic API key
    println!("\n  Anthropic API Key (required)");
    println!("  Get one at: https://console.anthropic.com/settings/keys\n");

    let api_key = if let Ok(existing) = std::env::var("ANTHROPIC_API_KEY") {
        if !existing.is_empty() && existing.starts_with("sk-ant-") {
            println!("  Found ANTHROPIC_API_KEY in environment.");
            existing
        } else {
            prompt_api_key()?
        }
    } else {
        prompt_api_key()?
    };

    // Step 3: Write API key to shell RC
    let shell_rc = detect_shell_rc();
    if let Some(rc_path) = &shell_rc {
        let rc_content = std::fs::read_to_string(rc_path).unwrap_or_default();
        if !rc_content.contains("ANTHROPIC_API_KEY") {
            let mut file = std::fs::OpenOptions::new().append(true).open(rc_path)?;
            writeln!(file, "\nexport ANTHROPIC_API_KEY=\"{}\"", api_key)?;
            println!("  Saved to {}", rc_path.display());
        }
    }
    std::env::set_var("ANTHROPIC_API_KEY", &api_key);

    // Step 4: Optional Tavily key
    println!("\n  Tavily API Key (optional — enables web search)");
    println!("  Get one at: https://app.tavily.com/home");
    println!("  Press Enter to skip.\n");

    print!("  API key: ");
    io::stdout().flush()?;
    let mut tavily_key = String::new();
    io::stdin().lock().read_line(&mut tavily_key)?;
    let tavily_key = tavily_key.trim().to_string();

    if !tavily_key.is_empty() {
        if let Some(rc_path) = &shell_rc {
            let rc_content = std::fs::read_to_string(rc_path).unwrap_or_default();
            if !rc_content.contains("TAVILY_API_KEY") {
                let mut file = std::fs::OpenOptions::new().append(true).open(rc_path)?;
                writeln!(file, "export TAVILY_API_KEY=\"{}\"", tavily_key)?;
            }
        }
        std::env::set_var("TAVILY_API_KEY", &tavily_key);
        println!("  Saved.");
    } else {
        println!("  Skipped — web_search tool won't be available.");
    }

    // Step 5: Verify
    println!("\n  Verifying API connection...");
    let api = meepo_core::api::ApiClient::new(
        api_key,
        Some("claude-sonnet-4-5-20250929".to_string()),
    );
    match api.send_message("Say 'hello' in one word.", &[], None).await {
        Ok(response) => {
            let text: String = response.content.iter()
                .filter_map(|b| if let meepo_core::api::ContentBlock::Text { text } = b { Some(text.as_str()) } else { None })
                .collect();
            println!("  Response: {}", text.trim());
            println!("  ✓ API connection works!\n");
        }
        Err(e) => {
            eprintln!("  ✗ API test failed: {}", e);
            eprintln!("  Check your API key and try again.\n");
        }
    }

    // Summary
    println!("  Setup complete!");
    println!("  ─────────────");
    println!("  Config:  {}", config_path.display());
    println!("  Soul:    {}", config_dir.join("workspace/SOUL.md").display());
    println!("  Memory:  {}", config_dir.join("workspace/MEMORY.md").display());
    println!();
    println!("  Next steps:");
    println!("    meepo start          # start the daemon");
    println!("    meepo ask \"Hello\"    # one-shot question");
    println!("    nano {}  # enable channels", config_path.display());
    println!();

    Ok(())
}

fn prompt_api_key() -> Result<String> {
    use std::io::{self, Write, BufRead};
    loop {
        print!("  API key (sk-ant-...): ");
        io::stdout().flush()?;
        let mut key = String::new();
        io::stdin().lock().read_line(&mut key)?;
        let key = key.trim().to_string();
        if key.starts_with("sk-ant-") {
            return Ok(key);
        }
        if key.is_empty() {
            anyhow::bail!("API key is required. Get one at https://console.anthropic.com/settings/keys");
        }
        println!("  Key should start with 'sk-ant-'. Try again.");
    }
}

fn detect_shell_rc() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let shell = std::env::var("SHELL").unwrap_or_default();
    if shell.contains("zsh") {
        Some(home.join(".zshrc"))
    } else if shell.contains("bash") {
        let bashrc = home.join(".bashrc");
        let profile = home.join(".bash_profile");
        if profile.exists() { Some(profile) } else { Some(bashrc) }
    } else {
        None
    }
}
```

**Step 3: Verify it compiles**

```bash
cargo check -p meepo-cli
```

**Step 4: Test the command**

```bash
cargo run -- setup --help
```

Expected: Shows "Interactive first-time setup wizard" in help output.

**Step 5: Commit**

```bash
git add crates/meepo-cli/src/main.rs
git commit -m "feat: add 'meepo setup' interactive wizard command"
```

---

### Task 2: Create GitHub Actions release workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Step 1: Create the workflow file**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    strategy:
      matrix:
        include:
          - target: aarch64-apple-darwin
            os: macos-latest
            name: meepo-darwin-arm64
          - target: x86_64-apple-darwin
            os: macos-latest
            name: meepo-darwin-x64
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            name: meepo-windows-x64

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Package (Unix)
        if: runner.os != 'Windows'
        run: |
          cd target/${{ matrix.target }}/release
          tar czf ../../../${{ matrix.name }}.tar.gz meepo
          cd ../../..
          shasum -a 256 ${{ matrix.name }}.tar.gz > ${{ matrix.name }}.tar.gz.sha256

      - name: Package (Windows)
        if: runner.os == 'Windows'
        run: |
          cd target/${{ matrix.target }}/release
          7z a ../../../${{ matrix.name }}.zip meepo.exe
          cd ../../..
          certutil -hashfile ${{ matrix.name }}.zip SHA256 > ${{ matrix.name }}.zip.sha256

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.name }}
          path: |
            ${{ matrix.name }}.tar.gz
            ${{ matrix.name }}.tar.gz.sha256
            ${{ matrix.name }}.zip
            ${{ matrix.name }}.zip.sha256

  release:
    needs: build
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Create release
        uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          files: |
            artifacts/*
```

**Step 2: Verify the YAML is valid**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" 2>/dev/null || echo "Install pyyaml to validate, or just check syntax visually"
```

**Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add GitHub Actions release workflow for multi-platform builds"
```

---

### Task 3: Create the universal install script

This is the main deliverable — a single script users `curl | bash` to install Meepo.

**Files:**
- Create: `install.sh` (repo root — short URL friendly)

**Step 1: Write the install script**

```bash
#!/bin/bash
set -euo pipefail

# Meepo Installer
# Usage: curl -sSL https://raw.githubusercontent.com/kavymi/meepo/main/install.sh | bash

REPO="kavymi/meepo"
INSTALL_DIR="${MEEPO_INSTALL_DIR:-$HOME/.local/bin}"

# ── Detect platform ──────────────────────────────────────────────

detect_platform() {
    local os arch

    case "$(uname -s)" in
        Darwin) os="darwin" ;;
        Linux)  os="linux" ;;
        MINGW*|MSYS*|CYGWIN*) os="windows" ;;
        *)
            echo "Error: Unsupported OS: $(uname -s)"
            echo "Meepo supports macOS and Windows."
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        arm64|aarch64) arch="arm64" ;;
        x86_64|amd64)  arch="x64" ;;
        *)
            echo "Error: Unsupported architecture: $(uname -m)"
            exit 1
            ;;
    esac

    if [ "$os" = "linux" ]; then
        echo ""
        echo "Note: Meepo on Linux has limited functionality."
        echo "Email, calendar, and UI automation tools require macOS or Windows."
        echo ""
    fi

    echo "meepo-${os}-${arch}"
}

# ── Find latest version ──────────────────────────────────────────

get_latest_version() {
    local version
    version=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed 's/.*"tag_name": *"//;s/".*//')

    if [ -z "$version" ]; then
        echo "Error: Could not determine latest version."
        echo "Check https://github.com/${REPO}/releases"
        exit 1
    fi
    echo "$version"
}

# ── Main ─────────────────────────────────────────────────────────

main() {
    echo ""
    echo "  Meepo Installer"
    echo "  ────────────────"
    echo ""

    local platform version url archive

    platform=$(detect_platform)
    echo "  Platform: ${platform}"

    version=$(get_latest_version)
    echo "  Version:  ${version}"

    if [[ "$platform" == *"windows"* ]]; then
        archive="${platform}.zip"
    else
        archive="${platform}.tar.gz"
    fi

    url="https://github.com/${REPO}/releases/download/${version}/${archive}"
    echo "  URL:      ${url}"
    echo ""

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Download and extract
    echo "  Downloading..."
    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" EXIT

    curl -sSL "$url" -o "$tmpdir/$archive"

    echo "  Extracting..."
    if [[ "$archive" == *.tar.gz ]]; then
        tar xzf "$tmpdir/$archive" -C "$tmpdir"
        mv "$tmpdir/meepo" "$INSTALL_DIR/meepo"
        chmod +x "$INSTALL_DIR/meepo"
    else
        unzip -q "$tmpdir/$archive" -d "$tmpdir"
        mv "$tmpdir/meepo.exe" "$INSTALL_DIR/meepo.exe"
    fi

    echo "  Installed to: $INSTALL_DIR/meepo"

    # Check PATH
    if ! echo "$PATH" | tr ':' '\n' | grep -q "^${INSTALL_DIR}$"; then
        echo ""
        echo "  ⚠ $INSTALL_DIR is not in your PATH."
        echo ""
        local shell_rc=""
        case "${SHELL:-}" in
            */zsh)  shell_rc="$HOME/.zshrc" ;;
            */bash) shell_rc="$HOME/.bashrc" ;;
        esac
        if [ -n "$shell_rc" ]; then
            echo "  Add it now? This appends to $shell_rc"
            printf "  [Y/n] "
            read -r yn </dev/tty
            if [ "${yn:-Y}" != "n" ] && [ "${yn:-Y}" != "N" ]; then
                echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$shell_rc"
                export PATH="$INSTALL_DIR:$PATH"
                echo "  ✓ Added to $shell_rc"
            fi
        else
            echo "  Add this to your shell profile:"
            echo "    export PATH=\"$INSTALL_DIR:\$PATH\""
        fi
    fi

    echo ""
    echo "  ✓ Meepo ${version} installed!"
    echo ""

    # Run setup
    printf "  Run interactive setup now? [Y/n] "
    read -r yn </dev/tty
    if [ "${yn:-Y}" != "n" ] && [ "${yn:-Y}" != "N" ]; then
        echo ""
        "$INSTALL_DIR/meepo" setup
    else
        echo ""
        echo "  Next steps:"
        echo "    meepo setup          # interactive setup wizard"
        echo "    meepo init           # just create config (no wizard)"
        echo "    meepo ask \"Hello\"    # one-shot question"
        echo ""
    fi
}

main
```

**Step 2: Make it executable and test syntax**

```bash
chmod +x install.sh
bash -n install.sh
```

**Step 3: Commit**

```bash
git add install.sh
git commit -m "feat: add universal curl installer script"
```

---

### Task 4: Create Windows install script (PowerShell)

**Files:**
- Create: `install.ps1` (repo root — for `irm | iex` pattern)

**Step 1: Write the PowerShell installer**

```powershell
#Requires -Version 5.1
$ErrorActionPreference = "Stop"

# Meepo Installer for Windows
# Usage: irm https://raw.githubusercontent.com/kavymi/meepo/main/install.ps1 | iex

$Repo = "kavymi/meepo"
$InstallDir = if ($env:MEEPO_INSTALL_DIR) { $env:MEEPO_INSTALL_DIR } else { Join-Path $env:USERPROFILE ".local\bin" }

Write-Host ""
Write-Host "  Meepo Installer" -ForegroundColor Blue
Write-Host "  ────────────────"
Write-Host ""

# Detect platform
$arch = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
$platform = "meepo-windows-${arch}"
Write-Host "  Platform: $platform"

# Get latest version
$release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$version = $release.tag_name
Write-Host "  Version:  $version"

$archive = "$platform.zip"
$url = "https://github.com/$Repo/releases/download/$version/$archive"
Write-Host "  URL:      $url"
Write-Host ""

# Create install directory
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# Download and extract
Write-Host "  Downloading..."
$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

try {
    Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmpDir $archive)

    Write-Host "  Extracting..."
    Expand-Archive -Path (Join-Path $tmpDir $archive) -DestinationPath $tmpDir -Force
    Move-Item (Join-Path $tmpDir "meepo.exe") (Join-Path $InstallDir "meepo.exe") -Force

    Write-Host "  Installed to: $InstallDir\meepo.exe"
} finally {
    Remove-Item $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}

# Check PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$InstallDir*") {
    Write-Host ""
    Write-Host "  Adding $InstallDir to your PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$userPath", "User")
    $env:Path = "$InstallDir;$env:Path"
    Write-Host "  ✓ Added to User PATH"
}

Write-Host ""
Write-Host "  ✓ Meepo $version installed!" -ForegroundColor Green
Write-Host ""

# Run setup
$yn = Read-Host "  Run interactive setup now? [Y/n]"
if ($yn -ne "n" -and $yn -ne "N") {
    Write-Host ""
    & (Join-Path $InstallDir "meepo.exe") setup
} else {
    Write-Host ""
    Write-Host "  Next steps:"
    Write-Host "    meepo setup          # interactive setup wizard"
    Write-Host "    meepo init           # just create config (no wizard)"
    Write-Host '    meepo ask "Hello"    # one-shot question'
    Write-Host ""
}
```

**Step 2: Commit**

```bash
git add install.ps1
git commit -m "feat: add Windows PowerShell installer script"
```

---

### Task 5: Create Homebrew formula

This goes in the Meepo repo for now. The user needs to create a `homebrew-tap` repo on GitHub separately, but the formula file lives here for maintenance.

**Files:**
- Create: `Formula/meepo.rb`

**Step 1: Write the formula**

```ruby
class Meepo < Formula
  desc "Local AI agent — connects Claude to your email, calendar, and more"
  homepage "https://github.com/kavymi/meepo"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/kavymi/meepo/releases/download/v#{version}/meepo-darwin-arm64.tar.gz"
      # sha256 will be filled after first release
      sha256 "PLACEHOLDER"
    else
      url "https://github.com/kavymi/meepo/releases/download/v#{version}/meepo-darwin-x64.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "meepo"
  end

  def caveats
    <<~EOS
      To get started, run the interactive setup:
        meepo setup

      Or initialize manually:
        meepo init
        export ANTHROPIC_API_KEY="sk-ant-..."
        meepo start
    EOS
  end

  test do
    assert_match "Meepo", shell_output("#{bin}/meepo --version")
  end
end
```

**Step 2: Commit**

```bash
git add Formula/meepo.rb
git commit -m "feat: add Homebrew formula for tap distribution"
```

---

### Task 6: Add workflow to auto-update Homebrew formula SHA on release

**Files:**
- Modify: `.github/workflows/release.yml`

**Step 1: Add a job that updates the formula SHA256 values**

Add this job after the `release` job in `release.yml`:

```yaml
  update-homebrew:
    needs: release
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: main
          token: ${{ secrets.GITHUB_TOKEN }}

      - name: Download artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Update formula
        run: |
          VERSION="${GITHUB_REF_NAME#v}"
          ARM_SHA=$(cat artifacts/meepo-darwin-arm64.tar.gz.sha256 | awk '{print $1}')
          X64_SHA=$(cat artifacts/meepo-darwin-x64.tar.gz.sha256 | awk '{print $1}')

          sed -i '' "s/version \".*\"/version \"${VERSION}\"/" Formula/meepo.rb

          # Update ARM SHA (first PLACEHOLDER/sha256 after arm?)
          # Use python for reliable multi-line replacement
          python3 - <<PYEOF
          import re
          with open("Formula/meepo.rb") as f:
              content = f.read()
          # Replace first sha256 after arm? block
          content = re.sub(
              r'(Hardware::CPU\.arm\?.*?sha256 )"[^"]*"',
              f'\\1"{ARM_SHA}"',
              content, count=1, flags=re.DOTALL)
          # Replace second sha256 (x64)
          content = re.sub(
              r'(url.*darwin-x64.*?\n\s*sha256 )"[^"]*"',
              f'\\1"{X64_SHA}"',
              content, count=1, flags=re.DOTALL)
          with open("Formula/meepo.rb", "w") as f:
              f.write(content)
          PYEOF

      - name: Commit updated formula
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Formula/meepo.rb
          git diff --cached --quiet || git commit -m "chore: update Homebrew formula to ${GITHUB_REF_NAME}"
          git push
```

**Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: auto-update Homebrew formula SHA on release"
```

---

### Task 7: Update README with new install methods

**Files:**
- Modify: `README.md`

**Step 1: Add Install section before Setup Guide**

Replace the current Quick Start section with:

```markdown
## Install

**macOS / Linux (curl):**
```bash
curl -sSL https://raw.githubusercontent.com/kavymi/meepo/main/install.sh | bash
```

**macOS (Homebrew):**
```bash
brew install kavymi/tap/meepo
meepo setup
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/kavymi/meepo/main/install.ps1 | iex
```

**From source:**
```bash
git clone https://github.com/kavymi/meepo.git && cd meepo
cargo build --release && ./target/release/meepo setup
```

All methods run `meepo setup` — an interactive wizard that configures your API keys and tests the connection.
```

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add one-line install commands to README"
```

---

### Task 8: Tag first release and verify

**Step 1: Tag and push**

```bash
git tag v0.1.0
git push origin v0.1.0
```

**Step 2: Wait for GitHub Actions to complete**

Go to https://github.com/kavymi/meepo/actions and watch the release workflow.

Expected: 3 builds (macOS ARM64, macOS x64, Windows x64) → release created with 6 assets (3 archives + 3 SHA256 files).

**Step 3: Verify the install script works**

```bash
# In a clean directory (not the repo)
curl -sSL https://raw.githubusercontent.com/kavymi/meepo/main/install.sh | bash
```

Expected: Downloads binary, installs to `~/.local/bin/meepo`, offers to run setup.

**Step 4: Set up Homebrew tap**

Create a new repo `kavymi/homebrew-tap` on GitHub, then:

```bash
mkdir -p /tmp/homebrew-tap && cd /tmp/homebrew-tap
git init
cp /path/to/meepo/Formula/meepo.rb .
git add meepo.rb
git commit -m "Add meepo formula"
git remote add origin https://github.com/kavymi/homebrew-tap.git
git push -u origin main
```

**Step 5: Test Homebrew install**

```bash
brew install kavymi/tap/meepo
meepo --version
```

Expected: Installs and shows version.
