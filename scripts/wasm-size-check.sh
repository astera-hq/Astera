#!/usr/bin/env bash
# Checks built contract WASM binaries against contracts/.wasm-size-baseline.json.
#
# For each *.wasm in $WASM_DIR (default: target/wasm32-unknown-unknown/release),
# runs it through `wasm-opt -Oz` if available (falls back to the raw binary
# otherwise) and compares the resulting size to the recorded baseline. Fails
# if any contract grows by more than 10% over its baseline. Contracts not yet
# in the baseline are reported but don't fail the run — add them to the
# baseline file once their size is intentional.
#
# Usage: scripts/wasm-size-check.sh [WASM_DIR]
set -euo pipefail

WASM_DIR="${1:-target/wasm32-unknown-unknown/release}"
BASELINE_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/contracts/.wasm-size-baseline.json"
MAX_GROWTH_PCT=10
OPT_DIR="$(mktemp -d)"
trap 'rm -rf "$OPT_DIR"' EXIT

if [ ! -d "$WASM_DIR" ]; then
  echo "ERROR: $WASM_DIR does not exist — build the contracts first" >&2
  exit 1
fi

HAVE_WASM_OPT=0
if command -v wasm-opt >/dev/null 2>&1; then
  HAVE_WASM_OPT=1
else
  echo "WARNING: wasm-opt not found on PATH — comparing unoptimized binary sizes" >&2
fi

baseline_size() {
  local name="$1"
  python3 -c "
import json, sys
with open('$BASELINE_FILE') as f:
    data = json.load(f)
print(data.get('$name', ''))
"
}

failed=0
{
  echo "| Contract | Size (bytes) | Baseline | Change |"
  echo "|---|---|---|---|"
} > "$OPT_DIR/summary.md"

for wasm in "$WASM_DIR"/*.wasm; do
  [ -e "$wasm" ] || continue
  name="$(basename "$wasm" .wasm)"

  if [ "$HAVE_WASM_OPT" -eq 1 ]; then
    opt_wasm="$OPT_DIR/${name}.wasm"
    wasm-opt -Oz --enable-bulk-memory --enable-sign-ext -o "$opt_wasm" "$wasm"
    size=$(wc -c < "$opt_wasm" | tr -d ' ')
  else
    size=$(wc -c < "$wasm" | tr -d ' ')
  fi

  base="$(baseline_size "$name")"

  if [ -z "$base" ]; then
    echo "$name: $size bytes (no baseline recorded — add it to contracts/.wasm-size-baseline.json)"
    echo "| $name | $size | — | new |" >> "$OPT_DIR/summary.md"
    continue
  fi

  limit=$(( base + (base * MAX_GROWTH_PCT) / 100 ))
  pct_change=$(( (size - base) * 100 / base ))

  if [ "$size" -gt "$limit" ]; then
    echo "ERROR: $name grew from $base to $size bytes (+${pct_change}%), exceeding the ${MAX_GROWTH_PCT}% budget" >&2
    echo "| $name | $size | $base | +${pct_change}% ⚠️ |" >> "$OPT_DIR/summary.md"
    failed=1
  else
    echo "$name: $size bytes (baseline $base, ${pct_change}% change)"
    echo "| $name | $size | $base | ${pct_change}% |" >> "$OPT_DIR/summary.md"
  fi
done

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat "$OPT_DIR/summary.md" >> "$GITHUB_STEP_SUMMARY"
fi

exit "$failed"
