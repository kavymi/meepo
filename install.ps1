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
$cpuArch = (Get-CimInstance Win32_Processor).Architecture
# Architecture: 0=x86, 5=ARM, 9=x64, 12=ARM64
$arch = switch ($cpuArch) {
    12    { "arm64" }
    5     { "arm64" }
    9     { "x64" }
    default { "x64" }
}
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
    Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmpDir $archive) -ErrorAction Stop
} catch {
    Write-Host ""
    Write-Host "  Error: Failed to download from $url" -ForegroundColor Red
    Write-Host "  Check your internet connection and try again."
    Write-Host "  Releases: https://github.com/$Repo/releases"
    Remove-Item $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}

try {
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
    Write-Host "  Added to User PATH"
}

Write-Host ""
Write-Host "  Meepo $version installed!" -ForegroundColor Green
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
