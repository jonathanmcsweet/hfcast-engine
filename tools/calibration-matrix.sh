#!/usr/bin/env bash
#
# Runs the full cross-month calibration matrix.
#
# For every month, fits the shrink factor on that month alone and tests it on
# every other month. A factor that only works on the month it was fitted to is
# an overfit; a factor that holds across seasons and solar levels is safe to
# ship. The June-fitted column is the one that matters for the server, and the
# rest of the matrix is the evidence that the choice of fitting month barely
# matters — or the warning that it does.
set -euo pipefail

cd "$(dirname "$0")/.."
MONTHS=(2025-06 2025-07 2025-03 2024-12 2019-06 2019-12)
OUT="${1:-docs/calibration-matrix.md}"

# Dump filename inside each month's directory. `hours.csv` is the standard
# configuration; `hours-es.csv` is the one with VOACAP's sporadic-E term on.
DUMP="${2:-hours.csv}"

BIN=./target/release/calibrate
if [ ! -x "$BIN" ]; then
  echo "build first: cargo build --release" >&2
  exit 1
fi

{
  echo "# Cross-month calibration matrix"
  echo
  for fit in "${MONTHS[@]}"; do
    args=(--fit "data/$fit/$DUMP")
    for test in "${MONTHS[@]}"; do
      if [ "$test" != "$fit" ]; then
        args+=(--test "data/$test/$DUMP")
      fi
    done
    "$BIN" "${args[@]}"
    echo
  done
} > "$OUT"

echo "wrote $OUT" >&2
