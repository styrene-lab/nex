#!/usr/bin/env bash
# Test: config resolution priority — CLI > config file > discovery.
source "$HARNESS"

# ── Config file resolution ────────────────────────────────────────────────

setup_clean_home
repo=$(mktemp -d)
setup_repo "$repo"
setup_nex_config "$repo"

assert_output_contains "config file points to correct repo" \
  "git" \
  nex list

# ── CLI --repo overrides config file ─────────────────────────────────────

other_repo=$(mktemp -d)
setup_repo "$other_repo"

# Add a unique package to the other repo so we can tell them apart
echo "    btop" >> "$other_repo/nix/modules/home/base.nix"
# Fix: insert before the closing ];
sed -i 's/^  ];/    btop\n  ];/' "$other_repo/nix/modules/home/base.nix"
(cd "$other_repo" && git add -A && git commit -q -m "add btop")

# btop should appear when using --repo override
assert_output_contains "--repo overrides config file" \
  "btop" \
  nex --repo "$other_repo" list

# btop should NOT appear when using the default config
assert_output_not_contains "default config doesn't have btop" \
  "btop" \
  nex list

# ── ENV var NEX_REPO overrides config file ────────────────────────────────

assert_output_contains "NEX_REPO env overrides config" \
  "btop" \
  env NEX_REPO="$other_repo" nex list

# ── Discovery when no config file ────────────────────────────────────────

setup_clean_home
# Put a darwin repo at ~/macos-nix (well-known path)
setup_repo "$HOME/macos-nix"

# Remove config file — force discovery
rm -f "$HOME/.config/nex/config.toml"

assert_output_contains "discovery finds ~/macos-nix" \
  "git" \
  nex list

# ── No config, no discoverable repo → error ──────────────────────────────

setup_clean_home
rm -f "$HOME/.config/nex/config.toml"

assert_fail "fails with no config and no discoverable repo" \
  nex list

finish
