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

# ─── Maintenance ────────────────────────────────────────────────────────────

# Clean build artifacts
clean:
    cargo clean
