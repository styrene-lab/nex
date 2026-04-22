#!/bin/sh
# nex installer — https://nex.styrene.io
# Usage: curl -fsSL https://nex.styrene.io/install.sh | sh
#
# POSIX-compatible — no bashisms. Safe under dash, bash, zsh, sh.
set -eu

NEX_REPO="https://github.com/styrene-lab/nex"
NEX_VERSION="${NEX_VERSION:-latest}"
_TMPDIR=""

# ─── Colors ─────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
  C='\033[36m' G='\033[32m' Y='\033[33m' R='\033[31m' B='\033[1m' N='\033[0m'
else
  C='' G='' Y='' R='' B='' N=''
fi

info()  { printf "%b>>>%b %s\n" "$C" "$N" "$*"; }
ok()    { printf " %b✓%b %s\n" "$G" "$N" "$*"; }
warn()  { printf " %b!%b %s\n" "$Y" "$N" "$*"; }
fail()  { printf " %b✗%b %s\n" "$R" "$N" "$*" >&2; exit 1; }

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
  aarch64-Linux)               TARGET="aarch64-unknown-linux-musl" ;;
  x86_64-Linux)                TARGET="x86_64-unknown-linux-musl" ;;
  *) fail "Unsupported platform: ${ARCH}-${OS}" ;;
esac

# Detect NixOS — dynamically linked binaries won't work
IS_NIXOS=false
if [ -f /etc/NIXOS ]; then
  IS_NIXOS=true
fi

# ─── Cleanup ────────────────────────────────────────────────────────────────
cleanup() {
  if [ -n "$_TMPDIR" ] && [ -d "$_TMPDIR" ]; then
    rm -rf "$_TMPDIR"
  fi
}
trap cleanup EXIT

# ─── Install Methods ────────────────────────────────────────────────────────

try_prebuilt() {
  if [ "$NEX_VERSION" = "latest" ]; then
    url="${NEX_REPO}/releases/latest/download/nex-${TARGET}.tar.gz"
  else
    url="${NEX_REPO}/releases/download/v${NEX_VERSION}/nex-${TARGET}.tar.gz"
  fi

  # Quick HEAD check — don't download if the release doesn't exist
  if ! curl -fsSL --head "$url" >/dev/null 2>&1; then
    # Fallback: try gnu target if musl isn't available (and vice versa)
    case "$TARGET" in
      *-musl) fallback_target=$(echo "$TARGET" | sed 's/-musl$/-gnu/'); ;;
      *-gnu)  fallback_target=$(echo "$TARGET" | sed 's/-gnu$/-musl/'); ;;
      *)      return 1 ;;
    esac
    url="${NEX_REPO}/releases/latest/download/nex-${fallback_target}.tar.gz"
    if ! curl -fsSL --head "$url" >/dev/null 2>&1; then
      return 1
    fi
    if [ "$IS_NIXOS" = "true" ] && echo "$fallback_target" | grep -q "gnu"; then
      warn "Only glibc binary available — won't work on NixOS"
      return 1
    fi
  fi

  info "Downloading prebuilt binary for ${TARGET}..."
  _TMPDIR=$(mktemp -d)

  if ! curl -fsSL "$url" -o "$_TMPDIR/nex.tar.gz"; then
    warn "download failed"
    return 1
  fi

  if ! tar -xzf "$_TMPDIR/nex.tar.gz" -C "$_TMPDIR"; then
    warn "archive extraction failed"
    return 1
  fi

  if [ ! -f "$_TMPDIR/nex" ]; then
    warn "binary not found in archive"
    return 1
  fi

  install_dir="$(pick_install_dir)"

  # Try creating as the user first — avoids root-owning ~/.local
  if mkdir -p "$install_dir" 2>/dev/null && [ -w "$install_dir" ]; then
    mv "$_TMPDIR/nex" "$install_dir/nex"
    chmod +x "$install_dir/nex"
  else
    info "installing to ${install_dir} (sudo required)..."
    sudo mkdir -p "$install_dir" </dev/tty 2>/dev/null || mkdir -p "$install_dir"
    sudo mv "$_TMPDIR/nex" "$install_dir/nex" </dev/tty
    sudo chmod +x "$install_dir/nex" </dev/tty
  fi

  rm -rf "$_TMPDIR"
  _TMPDIR=""

  if [ ! -x "$install_dir/nex" ]; then
    warn "binary not found at $install_dir/nex after install"
    return 1
  fi

  ensure_path "$install_dir"
  printf "  installed to %s\n" "$install_dir"
  return 0
}

try_nix() {
  command -v nix >/dev/null 2>&1 || return 1

  flake="github:styrene-lab/nex"

  # Check if nex is already in the nix profile
  if nix profile list 2>/dev/null | grep -q 'styrene-lab/nex'; then
    installed_ver=$(nex --version 2>/dev/null | awk '{print $2}') || installed_ver="unknown"

    # Query the latest version from the flake
    latest_ver=$(nix eval "${flake}#default.version" --raw --refresh 2>/dev/null) || latest_ver=""

    if [ -n "$latest_ver" ] && [ "$installed_ver" != "$latest_ver" ]; then
      printf "  nex %b%s%b installed, %b%s%b available\n\n" \
        "$Y" "$installed_ver" "$N" "$G" "$latest_ver" "$N"
      printf "  Upgrade? [Y/n] "
      answer="y"
      if [ -t 0 ]; then
        read -r answer </dev/tty || answer="y"
        answer="${answer:-y}"
      fi
      case "$answer" in
        [Yy]*)
          printf "  "
          info "Upgrading nex ${installed_ver} -> ${latest_ver}..."
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

# Choose the best install directory.
pick_install_dir() {
  if [ -n "${NEX_INSTALL_DIR:-}" ]; then
    printf '%s' "$NEX_INSTALL_DIR"
    return
  fi

  # /usr/local/bin if writable (no sudo needed)
  if [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
    printf '%s' "/usr/local/bin"
    return
  fi

  printf '%s' "$HOME/.local/bin"
}

# Append a PATH export to a shell profile if not already present.
patch_profile() {
  profile="$1"
  dir="$2"
  marker='# Added by nex installer'

  # File must exist AND be writable (not a read-only symlink to nix store)
  [ -f "$profile" ] && [ -w "$profile" ] || return 1
  # Already patched
  grep -qF "$marker" "$profile" 2>/dev/null && return 0

  printf '\n%s\nexport PATH="%s:$PATH"\n' "$marker" "$dir" >> "$profile"
  return 0
}

ensure_path() {
  dir="$1"
  case ":$PATH:" in
    *":$dir:"*) return ;;  # already in PATH
  esac

  # Patch writable shell profiles
  patched=false
  for profile in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile" "$HOME/.zprofile"; do
    if patch_profile "$profile" "$dir"; then
      patched=true
    fi
  done

  # Last resort: create a profile file
  if [ "$patched" = "false" ]; then
    case "$OS" in
      Darwin) target="$HOME/.zprofile" ;;  # macOS default shell is zsh
      *)      target="$HOME/.profile" ;;
    esac
    if ! grep -qF '# Added by nex installer' "$target" 2>/dev/null; then
      printf '\n# Added by nex installer\nexport PATH="%s:$PATH"\n' "$dir" >> "$target"
    fi
  fi

  # Add to current session too
  export PATH="$dir:$PATH"
}

# ─── Main ────────────────────────────────────────────────────────────────────

printf "\n  %bnex%b — %bnex the briw%b\n\n" "$B" "$N" "$C" "$N"

# Detect what's available
has_nix=false; has_cargo=false
command -v nix   >/dev/null 2>&1 && has_nix=true
command -v cargo >/dev/null 2>&1 && has_cargo=true

# Install order depends on platform:
# NixOS: nix -> prebuilt (static musl) -> cargo
# Other: prebuilt -> nix -> cargo
installed=false

if [ "$IS_NIXOS" = "true" ]; then
  # NixOS: prefer nix profile install (guaranteed to work)
  if $has_nix && try_nix; then
    installed=true
  elif try_prebuilt; then
    installed=true
  elif $has_cargo && try_cargo; then
    installed=true
  fi
else
  if try_prebuilt; then
    installed=true
  elif $has_nix && try_nix; then
    installed=true
  elif $has_cargo && try_cargo; then
    installed=true
  fi
fi

if [ "$installed" = "false" ]; then
  printf "\n"
  printf " %b✗%b Could not install nex.\n\n" "$R" "$N"
  printf "  No prebuilt binary available for %b%s%b.\n" "$B" "$TARGET" "$N"
  if [ "$has_nix" = "false" ]; then
    printf "  %b!%b nix not found\n" "$Y" "$N"
  fi
  if [ "$has_cargo" = "false" ]; then
    printf "  %b!%b cargo not found\n" "$Y" "$N"
  fi
  printf "\n  Install one of these, then re-run:\n"
  printf "  * %bnix%b   — curl --proto '=https' -sSf -L https://install.determinate.systems/nix | sh\n" "$C" "$N"
  printf "  * %bcargo%b — curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh\n\n" "$C" "$N"
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
printf "  %bnex install htop%b        Install a package\n" "$C" "$N"
printf "  %bnex list%b               Show all packages\n" "$C" "$N"
printf "  %bnex --help%b             Full usage\n" "$C" "$N"
printf "  %bhttps://nex.styrene.io%b  Documentation\n\n" "$C" "$N"
