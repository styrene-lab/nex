# Multi-stage build: compile nex, then run integration tests with mock binaries.
# Usage: docker build -f tests/integration/Dockerfile -t nex-integration .
#        docker run --rm nex-integration

# ── Stage 1: Build ──────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release 2>&1 \
    && cp target/release/nex /usr/local/bin/nex

# ── Stage 2: Test runtime ──────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        git ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root test user
RUN useradd -m -s /bin/bash testuser
USER testuser
WORKDIR /home/testuser

# Install the nex binary
COPY --from=builder /usr/local/bin/nex /usr/local/bin/nex

# Install mock binaries (before real PATH entries)
COPY --chown=testuser:testuser tests/integration/mocks/ /home/testuser/.mocks/
RUN chmod +x /home/testuser/.mocks/*

# Install test harness and tests
COPY --chown=testuser:testuser tests/integration/harness.sh /home/testuser/harness.sh
COPY --chown=testuser:testuser tests/integration/run_all.sh /home/testuser/run_all.sh
COPY --chown=testuser:testuser tests/integration/tests/ /home/testuser/tests/
RUN chmod +x /home/testuser/run_all.sh /home/testuser/tests/*.sh

# Mocks go first in PATH so they shadow real binaries.
# Keep /usr/bin for git, bash, etc.
ENV PATH="/home/testuser/.mocks:/usr/local/bin:/usr/bin:/bin"
ENV HOME="/home/testuser"

# git needs identity for nex init scaffold commits
RUN git config --global user.name "test" && git config --global user.email "test@test"

ENTRYPOINT ["bash", "/home/testuser/run_all.sh"]
