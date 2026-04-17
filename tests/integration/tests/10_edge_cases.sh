#!/usr/bin/env bash
# Test: edge cases and error handling.
source "$HARNESS"

# ── Help and version ──────────────────────────────────────────────────────

assert_ok "nex --help succeeds" \
  nex --help

assert_output_contains "help shows package manager description" \
  "Package manager" \
  nex --help

assert_ok "nex --version succeeds" \
  nex --version

assert_output_contains "version shows nex" \
  "nex" \
  nex --version

# ── Invalid subcommand ───────────────────────────────────────────────────

assert_fail "invalid subcommand fails" \
  nex frobnicate

# ── Install then remove round-trip ────────────────────────────────────────

setup_clean_home
repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

assert_ok "install btop" \
  nex install --nix btop

assert_file_contains "btop present" \
  "$repo/nix/modules/home/base.nix" "btop"

assert_ok "remove btop" \
  nex remove btop

assert_file_not_contains "btop gone" \
  "$repo/nix/modules/home/base.nix" "btop"

# Original packages should be untouched
assert_file_contains "git still there after round-trip" \
  "$repo/nix/modules/home/base.nix" "git"
assert_file_contains "vim still there after round-trip" \
  "$repo/nix/modules/home/base.nix" "vim"

# ── List on empty repo (no packages) ─────────────────────────────────────

setup_clean_home
empty_repo=$(mktemp -d)
setup_repo "$empty_repo"

# Empty out the packages list
cat > "$empty_repo/nix/modules/home/base.nix" <<'NIX'
{ pkgs, username, ... }:

{
  home.packages = with pkgs; [
  ];
}
NIX

cat > "$empty_repo/nix/modules/darwin/homebrew.nix" <<'NIX'
{ ... }:

{
  homebrew = {
    enable = true;
    onActivation = {
      autoUpdate = true;
      upgrade = true;
      cleanup = "zap";
    };
    brews = [
    ];
    casks = [
    ];
  };
}
NIX

(cd "$empty_repo" && git add -A && git commit -q -m "empty")
setup_nex_config "$empty_repo"

assert_ok "list on empty config succeeds" \
  nex list

assert_output_contains "shows 0 nix packages" \
  "0 total" \
  nex list

# ── Install into empty repo works ────────────────────────────────────────

assert_ok "install into empty repo" \
  nex install --nix htop

assert_file_contains "htop added to empty base.nix" \
  "$empty_repo/nix/modules/home/base.nix" "htop"

finish
