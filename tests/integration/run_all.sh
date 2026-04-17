#!/usr/bin/env bash
# Run all integration tests and report results.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TEST_DIR="${1:-$SCRIPT_DIR/tests}"
HARNESS="$SCRIPT_DIR/harness.sh"

_green() { printf '\033[32m%s\033[0m' "$*"; }
_red()   { printf '\033[31m%s\033[0m' "$*"; }
_bold()  { printf '\033[1m%s\033[0m' "$*"; }

total_pass=0
total_fail=0
failed_tests=()

echo ""
echo "$(_bold "nex integration tests")"
echo ""

for test_file in "$TEST_DIR"/*.sh; do
  test_name=$(basename "$test_file" .sh)
  result_file=$(mktemp)

  echo "  $(_bold "$test_name")"

  if TEST_NAME="$test_name" \
     RESULT_FILE="$result_file" \
     HARNESS="$HARNESS" \
     bash "$test_file"; then
    :
  else
    # Test script itself failed (not just assertion failures)
    if [[ ! -s "$result_file" ]]; then
      echo "0 1" > "$result_file"
      echo "    $(_red FAIL) test script crashed"
    fi
  fi

  if [[ -s "$result_file" ]]; then
    read -r p f < "$result_file"
    total_pass=$((total_pass + p))
    total_fail=$((total_fail + f))
    if [[ $f -gt 0 ]]; then
      failed_tests+=("$test_name")
    fi
  fi

  rm -f "$result_file"
  echo ""
done

echo "──────────────────────────────────────"
echo "  total: $(_green "${total_pass} passed"), $(_red "${total_fail} failed")"

if [[ ${#failed_tests[@]} -gt 0 ]]; then
  echo ""
  echo "  failed:"
  for t in "${failed_tests[@]}"; do
    echo "    $(_red "•") $t"
  done
fi

echo ""
[[ $total_fail -eq 0 ]]
