#!/usr/bin/env bash
# nex installer — https://nex.styrene.io
# Usage: curl -fsSL https://nex.styrene.io/install.sh | sh
set -euo pipefail

NEX_REPO="https://github.com/styrene-lab/nex"
NEX_VERSION="${NEX_VERSION:-latest}"

# ─── Colors ─────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
  C='\033[36m' G='\033[32m' Y='\033[33m' R='\033[31m' B='\033[1m' N='\033[0m'
else
  C='' G='' Y='' R='' B='' N=''
fi

info()  { printf "${C}>>>${N} %s\n" "$*"; }
ok()    { printf " ${G}✓${N} %s\n" "$*"; }
warn()  { printf " ${Y}!${N} %s\n" "$*"; }
fail()  { printf " ${R}✗${N} %s\n" "$*" >&2; exit 1; }

# ─── Platform ───────────────────────────────────────────────────────────────
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Darwin) ;; # macOS — primary target
  Linux)  ;; # supported
  *)      fail "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64|aarch64|arm64) ;;
  *) fail "Unsupported architecture: $ARCH" ;;
esac

case "${ARCH}-${OS}" in
  arm64-Darwin|aarch64-Darwin) TARGET="aarch64-apple-darwin" ;;
  x86_64-Darwin)               TARGET="x86_64-apple-darwin" ;;
  aarch64-Linux)               TARGET="aarch64-unknown-linux-gnu" ;;
  x86_64-Linux)                TARGET="x86_64-unknown-linux-gnu" ;;
  *) fail "Unsupported platform: ${ARCH}-${OS}" ;;
esac

# ─── Install Methods ────────────────────────────────────────────────────────

try_prebuilt() {
  if [ "$NEX_VERSION" = "latest" ]; then
    local url="${NEX_REPO}/releases/latest/download/nex-${TARGET}.tar.gz"
  else
    local url="${NEX_REPO}/releases/download/v${NEX_VERSION}/nex-${TARGET}.tar.gz"
  fi

  # Quick HEAD check — don't download if the release doesn't exist
  if ! curl -fsSL --head "$url" >/dev/null 2>&1; then
    return 1
  fi

  info "Downloading prebuilt binary for ${TARGET}..."
  local tmpdir
  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' EXIT

  curl -fsSL "$url" -o "$tmpdir/nex.tar.gz"
  tar -xzf "$tmpdir/nex.tar.gz" -C "$tmpdir"

  local install_dir="${NEX_INSTALL_DIR:-$HOME/.local/bin}"
  mkdir -p "$install_dir"
  mv "$tmpdir/nex" "$install_dir/nex"
  chmod +x "$install_dir/nex"

  printf "  installed to %s\n" "$install_dir/nex"
  ensure_path "$install_dir"
  return 0
}

try_nix() {
  command -v nix >/dev/null 2>&1 || return 1

  local flake="github:styrene-lab/nex"

  # Check if nex is already in the nix profile
  if nix profile list 2>/dev/null | grep -q 'styrene-lab/nex'; then
    local installed_ver
    installed_ver=$(nex --version 2>/dev/null | awk '{print $2}') || installed_ver="unknown"

    # Query the latest version from the flake
    local latest_ver
    latest_ver=$(nix eval "${flake}#default.version" --raw --refresh 2>/dev/null) || latest_ver=""

    if [ -n "$latest_ver" ] && [ "$installed_ver" != "$latest_ver" ]; then
      printf "  nex ${Y}${installed_ver}${N} installed, ${G}${latest_ver}${N} available\n\n"
      printf "  Upgrade? [Y/n] "
      # Default to yes; non-interactive (piped) stdin also defaults yes
      local answer="y"
      if [ -t 0 ]; then
        read -r answer </dev/tty || answer="y"
        answer="${answer:-y}"
      fi
      case "$answer" in
        [Yy]*)
          info "Upgrading nex ${installed_ver} → ${latest_ver}..."
          nix profile remove '.*nex.*' 2>/dev/null || true
          nix profile add "$flake" --refresh 2>/dev/null \
            || nix profile install "$flake" --refresh
          return 0
          ;;
        *)
          info "Keeping nex ${installed_ver}"
          return 0
          ;;
      esac
    fi

    ok "nex ${installed_ver} is already the latest"
    return 0
  fi

  info "Installing via nix flake..."
  if nix profile add "$flake" --refresh 2>/dev/null; then
    return 0
  fi
  nix profile install "$flake" --refresh
  return 0
}

try_cargo() {
  command -v cargo >/dev/null 2>&1 || return 1

  info "Installing from crates.io..."
  if [ "$NEX_VERSION" = "latest" ]; then
    cargo install nex-pkg --quiet
  else
    cargo install nex-pkg --version "$NEX_VERSION" --quiet
  fi
  return 0
}

ensure_path() {
  local dir="$1"
  case ":$PATH:" in
    *":$dir:"*) ;;
    *)
      warn "$dir is not in your PATH"
      printf "\n  Add to your shell profile:\n"
      printf "  ${C}export PATH=\"\$PATH:%s\"${N}\n" "$dir"
      ;;
  esac
}

# ─── Main ────────────────────────────────────────────────────────────────────

printf "\n  ${B}nex${N} — ${C}nix the brew${N}\n\n"

# Detect what's available
has_nix=false; has_cargo=false
command -v nix   >/dev/null 2>&1 && has_nix=true
command -v cargo >/dev/null 2>&1 && has_cargo=true

# Try in order: prebuilt binary → nix → cargo
installed=false

if try_prebuilt 2>/dev/null; then
  installed=true
elif $has_nix && try_nix; then
  installed=true
elif $has_cargo && try_cargo; then
  installed=true
fi

if ! $installed; then
  printf "\n"
  fail "Could not install nex."
  printf "\n"
  printf "  Install one of:\n"
  printf "  • ${C}nix${N}   — https://determinate.systems/nix-installer\n"
  printf "  • ${C}cargo${N} — https://rustup.rs\n\n"
  exit 1
fi

# Verify
printf "\n"
if command -v nex >/dev/null 2>&1; then
  ok "nex $(nex --version 2>/dev/null | awk '{print $2}') installed"
else
  ok "nex installed (open a new shell to use it)"
fi

printf "\n  Get started:\n"
printf "  ${C}nex install htop${N}        Install a package\n"
printf "  ${C}nex list${N}               Show all packages\n"
printf "  ${C}nex --help${N}             Full usage\n"
printf "  ${C}https://nex.styrene.io${N}  Documentation\n\n"
