#!/usr/bin/env bash
# Test: nex remove removes packages from the correct files.
source "$HARNESS"
setup_clean_home

repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

# ── Remove a nix package ─────────────────────────────────────────────────

assert_file_contains "vim present before remove" \
  "$repo/nix/modules/home/base.nix" "vim"

assert_ok "remove vim succeeds" \
  nex remove vim

assert_file_not_contains "vim gone after remove" \
  "$repo/nix/modules/home/base.nix" "vim"

assert_file_contains "git still present" \
  "$repo/nix/modules/home/base.nix" "git"

# ── Remove a cask ────────────────────────────────────────────────────────

assert_ok "remove --cask slack succeeds" \
  nex remove --cask slack

assert_file_not_contains "slack gone after remove" \
  "$repo/nix/modules/darwin/homebrew.nix" '"slack"'

assert_file_contains "docker still present" \
  "$repo/nix/modules/darwin/homebrew.nix" '"docker"'

# ── Remove a brew formula ────────────────────────────────────────────────

assert_ok "remove --brew rustup succeeds" \
  nex remove --brew rustup

assert_file_not_contains "rustup gone after remove" \
  "$repo/nix/modules/darwin/homebrew.nix" '"rustup"'

# ── Remove non-existent package ──────────────────────────────────────────

assert_output_contains "removing missing package reports not found" \
  "not found" \
  nex remove nonexistent-pkg

finish
