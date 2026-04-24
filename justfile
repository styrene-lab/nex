# ─── Help ───────────────────────────────────────────────────────────────────
@default:
    just --list --unsorted

# ─── Development ────────────────────────────────────────────────────────────

# Run all checks
validate: format-check lint test

# Run tests
test:
    cargo test

# Run clippy
lint:
    cargo clippy --all-targets --no-deps -- -D warnings

# Check formatting
format-check:
    cargo fmt -- --check

# Format code
format:
    cargo fmt

# Type check
check:
    cargo check --all-targets

# ─── Build ──────────────────────────────────────────────────────────────────

# Debug build
build:
    cargo build

# Release build
build-release:
    cargo build --release

# Install to ~/.cargo/bin
install:
    cargo install --path .

# ─── Integration ───────────────────────────────────────────────────────

# Build and run integration tests in a container
integration:
    #!/usr/bin/env bash
    if command -v docker &>/dev/null && docker info &>/dev/null 2>&1; then
      engine=docker
    elif command -v podman &>/dev/null; then
      engine=podman
    else
      echo "error: docker or podman required" >&2; exit 1
    fi
    $engine build -f tests/integration/Dockerfile -t nex-integration .
    $engine run --rm nex-integration

# ─── Release ───────────────────────────────────────────────────────────────

# Read the current version from Cargo.toml
@version:
    grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'

# Bump version, update CHANGELOG, commit, tag, and push to trigger release.
# Usage: just release 0.14.0
release VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    CURRENT=$(just version)
    if [ "{{VERSION}}" = "$CURRENT" ]; then
      echo "error: already at {{VERSION}}" >&2; exit 1
    fi
    echo "  $CURRENT -> {{VERSION}}"
    # 1. Bump Cargo.toml (flake.nix reads from here automatically)
    sed -i '' "s/^version = \"$CURRENT\"/version = \"{{VERSION}}\"/" Cargo.toml
    cargo check --quiet 2>/dev/null  # regenerate Cargo.lock
    # 2. Check CHANGELOG has an entry (or add a stub)
    if ! grep -q '## \[{{VERSION}}\]' CHANGELOG.md; then
      DATE=$(date +%Y-%m-%d)
      # Insert after the header line
      sed -i '' "/^# Changelog/a\\
\\
## [{{VERSION}}] - ${DATE}\\
\\
### Changed\\
- (fill in before publishing)\\
" CHANGELOG.md
      # Add link reference
      sed -i '' "s|\[${CURRENT}\]: https://github.com/styrene-lab/nex/compare/|[{{VERSION}}]: https://github.com/styrene-lab/nex/compare/v${CURRENT}...v{{VERSION}}\n[${CURRENT}]: https://github.com/styrene-lab/nex/compare/|" CHANGELOG.md
      echo "  added CHANGELOG stub for {{VERSION}} — edit before tagging"
      echo "  run 'just tag' when CHANGELOG is ready"
    else
      echo "  CHANGELOG entry exists for {{VERSION}}"
      # Commit + tag + push
      just tag
    fi

# Commit version bump, tag, and push (triggers release workflow)
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(just version)
    if ! grep -q "## \[${VERSION}\]" CHANGELOG.md; then
      echo "error: CHANGELOG.md has no entry for ${VERSION}" >&2; exit 1
    fi
    git add Cargo.toml Cargo.lock CHANGELOG.md
    git commit -m "chore: bump version to ${VERSION}"
    git tag "v${VERSION}"
    git push origin main
    git push origin "v${VERSION}"
    echo "  v${VERSION} tagged and pushed — release workflow started"

# ─── Maintenance ────────────────────────────────────────────────────────────

# Clean build artifacts
clean:
    cargo clean
