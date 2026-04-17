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

# ─── Maintenance ────────────────────────────────────────────────────────────

# Clean build artifacts
clean:
    cargo clean
