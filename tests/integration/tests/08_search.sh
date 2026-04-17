#!/usr/bin/env bash
# Test: nex search, try, gc, diff, switch, update, rollback.
# These are thin wrappers over nix/darwin-rebuild — verify they invoke correctly.
source "$HARNESS"
setup_clean_home

repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

# ── Search ────────────────────────────────────────────────────────────────

assert_ok "search succeeds" \
  nex search htop

assert_output_contains "search shows results" \
  "htop" \
  nex search htop

# ── Try ───────────────────────────────────────────────────────────────────

assert_ok "try succeeds" \
  nex try htop

# ── Switch ────────────────────────────────────────────────────────────────

assert_ok "switch succeeds" \
  nex switch

assert_output_contains "switch invokes darwin-rebuild" \
  "darwin-rebuild switch" \
  nex switch

# ── Update ────────────────────────────────────────────────────────────────

assert_ok "update succeeds" \
  nex update

# ── Rollback ──────────────────────────────────────────────────────────────

assert_ok "rollback succeeds" \
  nex rollback

# ── Diff ���─────────────────────────────────────────────────────────────────

assert_ok "diff succeeds" \
  nex diff

# ── GC ────────────────────────────────────────────────────────────────────

assert_ok "gc succeeds" \
  nex gc

# ── Dry run on switch ───────────────────────────────────────────��────────

# Switch with --dry-run should not call darwin-rebuild
# (It should just show what would happen)
assert_ok "switch --dry-run succeeds" \
  nex switch --dry-run

finish
