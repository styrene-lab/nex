#!/usr/bin/env bash
# Test harness for nex integration tests.
# Each test script sources this, runs assertions, then calls `finish`.
set -euo pipefail

_PASS=0
_FAIL=0
_TEST_NAME="${TEST_NAME:-unknown}"

# ── Colors ──────────────────────────────────────────────────────────────────
_green() { printf '\033[32m%s\033[0m' "$*"; }
_red()   { printf '\033[31m%s\033[0m' "$*"; }
_bold()  { printf '\033[1m%s\033[0m' "$*"; }

# ── Assertions ──────────────────────────────────────────────────────────────

pass() { _PASS=$((_PASS + 1)); }
fail() {
  local desc="$1"; shift
  _FAIL=$((_FAIL + 1))
  echo "    $(_red FAIL) ${desc}"
  for line in "$@"; do echo "         ${line}"; done
}

# assert_ok <desc> <cmd...>
assert_ok() {
  local desc="$1"; shift
  if "$@" >/dev/null 2>&1; then pass
  else fail "$desc" "command: $*"; fi
}

# assert_fail <desc> <cmd...>
assert_fail() {
  local desc="$1"; shift
  if "$@" >/dev/null 2>&1; then
    fail "$desc (expected failure, got success)" "command: $*"
  else pass; fi
}

# assert_output_contains <desc> <needle> <cmd...>
assert_output_contains() {
  local desc="$1" needle="$2"; shift 2
  local output
  output=$("$@" 2>&1) || true
  if echo "$output" | grep -qF "$needle"; then pass
  else fail "$desc" "expected to contain: ${needle}" "got: $(echo "$output" | head -3)"; fi
}

# assert_output_not_contains <desc> <needle> <cmd...>
assert_output_not_contains() {
  local desc="$1" needle="$2"; shift 2
  local output
  output=$("$@" 2>&1) || true
  if echo "$output" | grep -qF "$needle"; then
    fail "$desc" "should not contain: ${needle}"
  else pass; fi
}

# assert_file_contains <desc> <file> <needle>
assert_file_contains() {
  local desc="$1" file="$2" needle="$3"
  if [[ ! -f "$file" ]]; then fail "$desc" "file not found: ${file}"; return; fi
  if grep -qF "$needle" "$file"; then pass
  else fail "$desc" "file: ${file}" "expected to contain: ${needle}"; fi
}

# assert_file_not_contains <desc> <file> <needle>
assert_file_not_contains() {
  local desc="$1" file="$2" needle="$3"
  if [[ ! -f "$file" ]]; then pass; return; fi
  if grep -qF "$needle" "$file"; then
    fail "$desc" "file: ${file}" "should not contain: ${needle}"
  else pass; fi
}

# assert_file_exists <desc> <file>
assert_file_exists() {
  local desc="$1" file="$2"
  if [[ -f "$file" ]]; then pass
  else fail "$desc" "not found: ${file}"; fi
}

# assert_eq <desc> <expected> <actual>
assert_eq() {
  local desc="$1" expected="$2" actual="$3"
  if [[ "$expected" == "$actual" ]]; then pass
  else fail "$desc" "expected: ${expected}" "actual:   ${actual}"; fi
}

# ── Test Setup ──────────────────────────────────────────────────────────────

# Create a fresh HOME for this test.
setup_clean_home() {
  local tmp
  tmp=$(mktemp -d)
  export HOME="$tmp"
  mkdir -p "$HOME/.config/nex"
  git config --global user.name "test" 2>/dev/null || true
  git config --global user.email "test@test" 2>/dev/null || true
}

# Write a nex config.toml pointing at the given repo.
setup_nex_config() {
  local repo="$1" hostname="${2:-test-host}"
  mkdir -p "$HOME/.config/nex"
  cat > "$HOME/.config/nex/config.toml" <<EOF
repo_path = "${repo}"
hostname = "${hostname}"
EOF
}

# Create a minimal nix-darwin repo nex can operate on.
setup_repo() {
  local repo="$1"
  mkdir -p "$repo/nix/modules/home" "$repo/nix/modules/darwin" \
           "$repo/nix/hosts/test-host" "$repo/nix/lib"

  cat > "$repo/flake.nix" <<'NIX'
{
  description = "test config";
  inputs = {};
  outputs = { self, ... }: {
    darwinConfigurations."test-host" = {};
  };
}
NIX

  cat > "$repo/nix/modules/home/base.nix" <<'NIX'
{ pkgs, username, ... }:

{
  home.packages = with pkgs; [
    git
    vim
  ];
}
NIX

  cat > "$repo/nix/modules/darwin/homebrew.nix" <<'NIX'
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
      "rustup"
    ];
    casks = [
      "slack"
      "docker"
    ];
  };
}
NIX

  (cd "$repo" && git init -q && git add -A && git commit -q -m "init")
}

# ── Finish ──────────────────────────────────────────────────────────────────

# Call at the end of each test script. Prints summary, exits.
finish() {
  if [[ $_FAIL -eq 0 ]]; then
    echo "    $(_green "✓ ${_PASS} passed")"
  else
    echo "    $(_red "✗ ${_FAIL} failed"), $(_green "${_PASS} passed")"
  fi

  # Write counts for the runner to read
  echo "${_PASS} ${_FAIL}" > "${RESULT_FILE:-/dev/null}"

  [[ $_FAIL -eq 0 ]]
}
