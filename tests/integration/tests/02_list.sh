#!/usr/bin/env bash
# Test: nex list displays declared packages correctly.
source "$HARNESS"
setup_clean_home

repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

# ── Basic list ────────────────────────────────────────────────────────────

assert_output_contains "lists nix packages header" \
  "Nix packages" \
  nex list

assert_output_contains "lists git" \
  "git" \
  nex list

assert_output_contains "lists vim" \
  "vim" \
  nex list

assert_output_contains "shows nix total" \
  "2 total" \
  nex list

assert_output_contains "lists brews header" \
  "Homebrew brews" \
  nex list

assert_output_contains "lists rustup brew" \
  "rustup" \
  nex list

assert_output_contains "lists casks header" \
  "Homebrew casks" \
  nex list

assert_output_contains "lists slack cask" \
  "slack" \
  nex list

assert_output_contains "lists docker cask" \
  "docker" \
  nex list

finish
