#!/usr/bin/env bash
# Test: auto-resolution and brew availability warnings.
source "$HARNESS"

# ── Nix-only package (no cask/formula) installs silently ──────────────────

setup_clean_home
repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

# htop is only in nixpkgs (not in brew mock) → auto picks nix
assert_ok "auto-resolve nix-only package" \
  nex install htop

assert_file_contains "htop added to nix packages" \
  "$repo/nix/modules/home/base.nix" "htop"

assert_file_not_contains "htop not in casks" \
  "$repo/nix/modules/darwin/homebrew.nix" "htop"

# ── Brew unavailable → warning on nix-only resolve ────────────────────────

setup_clean_home
repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

export MOCK_BREW_UNAVAILABLE=1
assert_output_contains "warns when brew unavailable" \
  "brew not available" \
  nex install btop
unset MOCK_BREW_UNAVAILABLE

# btop should still be installed (falls through to nix)
assert_file_contains "btop added despite brew warning" \
  "$repo/nix/modules/home/base.nix" "btop"

# ── Package not found anywhere → error ────────────────────────────────────

assert_fail "unknown package fails" \
  nex install totally-fake-package

# ── --nix flag skips resolution entirely ──────────────────────────────────

setup_clean_home
repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

# wget exists in brew formulae AND nixpkgs. --nix should bypass resolution.
assert_ok "install --nix wget skips resolution" \
  nex install --nix wget

assert_file_contains "wget in nix packages (not brew)" \
  "$repo/nix/modules/home/base.nix" "wget"

assert_file_not_contains "wget not in brews" \
  "$repo/nix/modules/darwin/homebrew.nix" '"wget"'

finish
