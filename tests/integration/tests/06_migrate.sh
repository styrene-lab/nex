#!/usr/bin/env bash
# Test: nex migrate reports brew packages correctly.
source "$HARNESS"
setup_clean_home

repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

# Mock brew data:
#   Installed leaves: wget, rustup, cmake
#   Managed brews:    rustup (in our config)
#   nixpkgs has:      wget (1.24.5), cmake (not in mock nixpkgs — only in brew)
#
# Expected:
#   wget → migrate candidate (in nixpkgs, not managed)
#   rustup → already managed
#   cmake → no nix equivalent (not in our mock nixpkgs db)
#   slack, spotify, docker (installed casks) — slack/docker managed, spotify unmanaged

assert_ok "migrate succeeds" \
  nex migrate

# wget is a brew leaf, exists in nixpkgs, not in nex config → candidate
assert_output_contains "wget shown as migrate candidate" \
  "wget" \
  nex migrate

# rustup is a brew leaf AND in nex config → already managed
assert_output_contains "rustup shown as managed" \
  "rustup" \
  nex migrate

assert_output_contains "shows managed section" \
  "Already managed" \
  nex migrate

# Installed cask spotify is NOT in our nex config → unmanaged
assert_output_contains "spotify shown as unmanaged cask" \
  "spotify" \
  nex migrate

assert_output_contains "shows unmanaged casks section" \
  "Unmanaged casks" \
  nex migrate

# ── Migrate with brew unavailable ────────────────────────────────────────

export MOCK_BREW_UNAVAILABLE=1
assert_output_contains "migrate reports brew not found" \
  "brew not found" \
  nex migrate
unset MOCK_BREW_UNAVAILABLE

finish
