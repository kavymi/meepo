# Homebrew Tap Setup

For `brew install leancoderkavy/tap/meepo` to work, you need a separate GitHub repo at `github.com/leancoderkavy/homebrew-tap`.

## One-Time Setup

### 1. Create the tap repo

```bash
# On GitHub: create a new public repo named "homebrew-tap" under the leancoderkavy org/user
# Or via gh CLI:
gh repo create leancoderkavy/homebrew-tap --public --description "Homebrew tap for Meepo"
```

### 2. Seed it with the formula

```bash
git clone https://github.com/leancoderkavy/homebrew-tap.git
cd homebrew-tap
mkdir -p Formula
cp /path/to/meepo/Formula/meepo.rb Formula/meepo.rb
```

Add a README:

```bash
cat > README.md << 'EOF'
# leancoderkavy/homebrew-tap

Homebrew formulae for [Meepo](https://github.com/leancoderkavy/meepo).

## Install

```bash
brew install leancoderkavy/tap/meepo
meepo setup
```

## Update

```bash
brew upgrade meepo
```

## Run as a service

```bash
brew services start meepo
```
EOF
```

```bash
git add .
git commit -m "Initial formula"
git push
```

### 3. Create a Personal Access Token for CI

The release workflow in the main repo needs to push formula updates to the tap repo.

1. Go to https://github.com/settings/tokens
2. Create a **Fine-grained token** scoped to the `leancoderkavy/homebrew-tap` repo
3. Grant **Contents: Read and write** permission
4. Copy the token

### 4. Add the secret to the main repo

```bash
# Via gh CLI:
gh secret set HOMEBREW_TAP_TOKEN --repo leancoderkavy/meepo
# Paste the token when prompted
```

Or: GitHub → leancoderkavy/meepo → Settings → Secrets and variables → Actions → New repository secret → Name: `HOMEBREW_TAP_TOKEN`

## How It Works

On every release tag (`v*`):

1. CI builds macOS binaries (arm64 + x64)
2. CI creates a GitHub Release with the tarballs
3. `update-homebrew` job:
   - Updates `Formula/meepo.rb` in the main repo (version + SHA256)
   - Clones `leancoderkavy/homebrew-tap`, copies the updated formula, pushes

Users then get the new version on their next `brew update && brew upgrade meepo`.

## Verifying

```bash
# After setup, test it works:
brew tap leancoderkavy/tap
brew install meepo

# Or in one command:
brew install leancoderkavy/tap/meepo
```

## Troubleshooting

- **`brew install` 404s**: The tap repo doesn't exist or the formula file isn't at `Formula/meepo.rb`
- **CI skips tap update**: `HOMEBREW_TAP_TOKEN` secret not set — check the workflow warning
- **Formula audit failures**: Run `brew audit --strict Formula/meepo.rb` locally before pushing
