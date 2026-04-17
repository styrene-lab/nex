#!/usr/bin/env bash
# Test: failed darwin-rebuild switch reverts file changes.
source "$HARNESS"
setup_clean_home

repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

# Capture the original file content
original=$(cat "$repo/nix/modules/home/base.nix")

# Make darwin-rebuild fail
export MOCK_SWITCH_FAIL=1

# Install should fail and revert
assert_fail "install fails when switch fails" \
  nex install --nix htop

# The file should be reverted to its original state
current=$(cat "$repo/nix/modules/home/base.nix")
assert_eq "base.nix reverted after failed switch" \
  "$original" "$current"

assert_file_not_contains "htop not in base.nix after revert" \
  "$repo/nix/modules/home/base.nix" "htop"

# ── Verify normal install works after revert ─────────────────────────────

unset MOCK_SWITCH_FAIL

assert_ok "install succeeds after revert" \
  nex install --nix htop

assert_file_contains "htop present after successful install" \
  "$repo/nix/modules/home/base.nix" "htop"

finish
