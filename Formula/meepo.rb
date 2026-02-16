class Meepo < Formula
  desc "Local AI agent — connects Claude to your email, calendar, and more"
  homepage "https://github.com/leancoderkavy/meepo"
  version "0.1.1"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/leancoderkavy/meepo/releases/download/v#{version}/meepo-darwin-arm64.tar.gz"
      sha256 "e60b7d93064c3e1fdcb4671afa45c331f7af29bc5bd4c8754d9f11bf08e590bc"
    else
      url "https://github.com/leancoderkavy/meepo/releases/download/v#{version}/meepo-darwin-x64.tar.gz"
      sha256 "3d0e21c52b485e127b97a43cfa5aa03f1791cdadf4a29baa8caa140aa6184adc"
    end
  end

  depends_on :macos

  def install
    bin.install "meepo"
  end

  def post_install
    # Initialize config directory and default config if not present.
    # This is non-destructive — skips if ~/.meepo/config.toml already exists.
    system bin/"meepo", "init" unless (Pathname.new(Dir.home)/".meepo"/"config.toml").exist?
  end

  # brew services support — `brew services start meepo` to run as a daemon
  service do
    run [opt_bin/"meepo", "start"]
    keep_alive true
    log_path var/"log/meepo/meepo.log"
    error_log_path var/"log/meepo/meepo-error.log"
    working_dir Dir.home
  end

  def caveats
    <<~EOS
      ┌──────────────────────────────────────────┐
      │  Get started in one command:              │
      │                                           │
      │    meepo setup                            │
      │                                           │
      │  This walks you through API keys, macOS   │
      │  permissions, and feature selection.       │
      └──────────────────────────────────────────┘

      After setup, start the agent:
        meepo start

      Or run as a background service (auto-starts on login):
        brew services start meepo

      Diagnose issues:
        meepo doctor

      Config file: ~/.meepo/config.toml
    EOS
  end

  test do
    assert_match "Meepo", shell_output("#{bin}/meepo --version")
  end
end
