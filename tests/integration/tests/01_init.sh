#!/usr/bin/env bash
# Test: nex init scaffolds a working nix-darwin config.
source "$HARNESS"
setup_clean_home

# ── Scaffold on a fresh system ────────────────────────────────────────────

# No existing config → init should scaffold at ~/macos-nix
assert_ok "init scaffold succeeds" \
  nex init

assert_file_exists "config.toml created" \
  "$HOME/.config/nex/config.toml"

assert_file_contains "config points to ~/macos-nix" \
  "$HOME/.config/nex/config.toml" \
  "repo_path = \"$HOME/macos-nix\""

assert_file_contains "config has hostname" \
  "$HOME/.config/nex/config.toml" \
  "hostname = \"test-host\""

assert_file_exists "flake.nix created" \
  "$HOME/macos-nix/flake.nix"

assert_file_contains "flake has darwinConfigurations" \
  "$HOME/macos-nix/flake.nix" \
  "darwinConfigurations"

assert_file_exists "base.nix created" \
  "$HOME/macos-nix/nix/modules/home/base.nix"

assert_file_contains "base.nix has home.packages" \
  "$HOME/macos-nix/nix/modules/home/base.nix" \
  "home.packages = with pkgs; ["

assert_file_exists "homebrew.nix created" \
  "$HOME/macos-nix/nix/modules/darwin/homebrew.nix"

assert_file_contains "homebrew.nix has brews list" \
  "$HOME/macos-nix/nix/modules/darwin/homebrew.nix" \
  "brews = ["

assert_file_contains "homebrew.nix has casks list" \
  "$HOME/macos-nix/nix/modules/darwin/homebrew.nix" \
  "casks = ["

# Scaffold should be a git repo with at least one commit
assert_ok "scaffold is a git repo" \
  git -C "$HOME/macos-nix" log --oneline -1

# ── Dry run ───────────────────────────────────────────────────────────────

setup_clean_home
assert_ok "init --dry-run succeeds" \
  nex init --dry-run

# Dry run should NOT create any files
assert_file_not_contains "no config.toml on dry-run" \
  "$HOME/.config/nex/config.toml" \
  "repo_path"

finish
