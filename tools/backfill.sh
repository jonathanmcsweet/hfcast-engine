#!/usr/bin/env bash
# Builds month bundles for a span of months and writes the whole-archive
# daily comparison CSV — one line per day, both engines scored against
# that day's ionosonde observations (`sonde --daily`).
#
# Resumable at every level: the GIRO fetch keeps files it already has
# (archive months do not change), the gather reads its month cache, and
# rerunning this script redoes only what is missing. The politeness
# rules hold — requests are spaced and the client identified — so the
# fetch phase is deliberately slow: about 100 spaced requests per new
# month. MUFD is left out here (the service has never answered it; the
# validation months keep asking so a change would surface).
#
# Phase 1 fetches every month serially (one polite stream). Phase 2
# gathers with a few parallel workers (engine runs, no network), each
# month writing its own cache file. Phase 3 writes the combined CSV.
#
# Usage: tools/backfill.sh 2015-01 2026-08 [out.csv]
set -euo pipefail

from="${1:?usage: backfill.sh YYYY-MM YYYY-MM [out.csv]}"
to="${2:?usage: backfill.sh YYYY-MM YYYY-MM [out.csv]}"
out="${3:-data/daily-comparison.csv}"

cd "$(dirname "$0")/.."

months=()
cursor="$from-01"
while [ "$(date -d "$cursor" +%Y-%m)" \< "$to" ] || [ "$(date -d "$cursor" +%Y-%m)" = "$to" ]; do
  months+=("$(date -d "$cursor" +%Y-%m)")
  cursor="$(date -d "$cursor +1 month" +%Y-%m-%d)"
done
echo "backfill: ${#months[@]} months, ${from}..${to}"

[ -s data/kp_daily.txt ] || tools/fetch-kp.sh

for month in "${months[@]}"; do
  # Present bundles are archive months; only fetch what is missing.
  if [ ! -s "data/${month}/giro/fetched.txt" ]; then
    GIRO_CHARS="foF2 foE hmF2 fmin" tools/fetch-giro.sh "${month}"
  fi
done

# The gather is CPU work only; four workers warm one month cache each
# (`--ledger` gathers and caches; its one printed line is not needed).
printf '%s\n' "${months[@]}" | xargs -P 4 -I{} sh -c '
  cargo run --release --all-features --bin sonde -- \
    --ledger --kp data/kp_daily.txt "data/$1" >/dev/null 2>&1 || true
' sh {}

# One serial pass over the warmed caches writes the combined CSV.
args=()
for month in "${months[@]}"; do
  [ -d "data/${month}/giro" ] && args+=("data/${month}")
done
cargo run --release --all-features --bin sonde -- \
  --daily --kp data/kp_daily.txt "${args[@]}" > "${out}"
echo "wrote ${out}: $(($(wc -l < "${out}") - 1)) day rows"
