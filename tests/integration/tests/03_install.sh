#!/usr/bin/env bash
# Test: nex install adds packages to the correct files.
source "$HARNESS"

# ── Install with --nix flag ───────────────────────────────────────────────

setup_clean_home
repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

assert_ok "install --nix htop succeeds" \
  nex install --nix htop

assert_file_contains "htop added to base.nix" \
  "$repo/nix/modules/home/base.nix" \
  "htop"

assert_output_contains "htop appears in list" \
  "htop" \
  nex list

# ── Install with --cask flag ─────────────────────────────────────────────

assert_ok "install --cask spotify succeeds" \
  nex install --cask spotify

assert_file_contains "spotify added to homebrew.nix casks" \
  "$repo/nix/modules/darwin/homebrew.nix" \
  '"spotify"'

# ── Install with --brew flag ─────────────────────────────────────────────

assert_ok "install --brew cmake succeeds" \
  nex install --brew cmake

assert_file_contains "cmake added to homebrew.nix brews" \
  "$repo/nix/modules/darwin/homebrew.nix" \
  '"cmake"'

# ── Duplicate detection ──────────────────────────────────────────────────

assert_output_contains "duplicate nix package detected" \
  "already present" \
  nex install --nix git

assert_output_contains "duplicate cask detected" \
  "already present" \
  nex install --cask slack

assert_output_contains "duplicate brew detected" \
  "already present" \
  nex install --brew rustup

# ── Multiple packages at once ────────────────────────────────────────────

setup_clean_home
repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

assert_ok "install multiple nix packages" \
  nex install --nix tmux jq wget

assert_file_contains "tmux in base.nix" \
  "$repo/nix/modules/home/base.nix" "tmux"
assert_file_contains "jq in base.nix" \
  "$repo/nix/modules/home/base.nix" "jq"
assert_file_contains "wget in base.nix" \
  "$repo/nix/modules/home/base.nix" "wget"

# ── Dry run doesn't edit ─────────────────────────────────────────────────

assert_ok "install --dry-run succeeds" \
  nex install --nix --dry-run curl

assert_file_not_contains "curl not added on dry-run" \
  "$repo/nix/modules/home/base.nix" "curl"

# ── Install with no packages errors ──────────────────────────────────────

assert_fail "install with no packages fails" \
  nex install

finish
