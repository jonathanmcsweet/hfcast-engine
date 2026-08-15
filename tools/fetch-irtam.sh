#!/usr/bin/env bash
# Downloads archived IRTAM coefficient files for one month, one file per day
# and characteristic, into data/<YYYY-MM>/irtam/.
#
# IRTAM refits the ionosphere map every 15 minutes from ionosonde data. The
# temporal terms describe the diurnal cycle over the trailing 24 hours, so
# the file from the end of a UT day (23:45) is the best single-file estimate
# of that whole day. See src/irtam.rs for the format.
#
# foF2 and hmF2 both come down. foF2 replaces the engine's foF2 map through
# the overlay; hmF2 needs the M(3000)F2 conversion recorded in docs/irtam.md
# before the engine can consume it, and is fetched now so the archive is on
# hand when that lands. The archive holds hmF2 as far back as foF2 (checked
# 2026-08-12 for 2015-03 and 2019-06).
#
# Usage: tools/fetch-irtam.sh 2025-06
set -euo pipefail

month="${1:?usage: fetch-irtam.sh YYYY-MM}"
year="${month%%-*}"
mm="${month##*-}"

cd "$(dirname "$0")/.."
out="data/${month}/irtam"
mkdir -p "$out"

base="https://ulcar.uml.edu/GAMBIT/GambitCoefficients/COEFFS_${year}"

fetched=0
missing=0
for char in foF2 hmF2; do
  for day in $(seq -w 1 31); do
    # Skip days the month does not have.
    date -d "${year}-${mm}-${day}" >/dev/null 2>&1 || continue
    file="IRTAM_${char}_COEFFS_${year}${mm}${day}_234500.ASC"
    dest="${out}/${file}"
    if [ -s "$dest" ]; then
      fetched=$((fetched + 1))
      continue
    fi
    url="${base}/${year}_${mm}_${day}/${file}"
    if curl -sf --max-time 60 -o "$dest" "$url"; then
      fetched=$((fetched + 1))
    else
      rm -f "$dest"
      missing=$((missing + 1))
      echo "missing: ${url}" >&2
    fi
    sleep 0.3
  done
done

echo "${month}: ${fetched} files fetched, ${missing} missing"
