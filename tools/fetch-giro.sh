#!/usr/bin/env bash
# Downloads scaled ionosonde characteristics for one month from GIRO
# FastChar, per station and characteristic, into data/<YYYY-MM>/giro/.
#
# Stations come from tools/giro-stations.tsv. Characteristics are foF2,
# foE, hmF2, MUFD (MUF for a 3000 km hop) and fmin (the ionogram's
# lowest returned frequency — the absorption proxy the NVIS lower edge
# scores against). See src/giro.rs for the format and the confidence
# rules. MUFD has never been served (see docs/roadmap.md); it stays in
# the list so a service-side change would surface by itself.
#
# GIRO is a research service. This script identifies itself, spaces its
# requests, fetches archive months once and keeps them (the past does not
# change), and writes the attribution the rules of the road ask for next
# to the data. A station-month the service does not have is an ordinary
# outcome, reported in the summary rather than failed on.
#
# Usage: tools/fetch-giro.sh 2025-06
set -euo pipefail

month="${1:?usage: fetch-giro.sh YYYY-MM}"
year="${month%%-*}"
mm="${month##*-}"

cd "$(dirname "$0")/.."
out="data/${month}/giro"
mkdir -p "$out"

base="https://lgdc.uml.edu/fastchar/getbest"
agent="hfcast-validation (github.com/jonathanmcsweet/hfcast-engine)"

# First day of the next month, for the range's open end.
next=$(date -d "${year}-${mm}-01 +1 month" +%Y.%m.01)

stations=0
files=0
empty=0
while IFS=$'\t' read -r ursi _name _lat _lon; do
  case "$ursi" in ''|'#'*) continue ;; esac
  stations=$((stations + 1))
  mkdir -p "${out}/${ursi}"
  for char in foF2 foE hmF2 MUFD fmin; do
    dest="${out}/${ursi}/${char}.txt"
    if [ -s "$dest" ]; then
      files=$((files + 1))
      continue
    fi
    url="${base}?ursiCode=${ursi}&charName=${char}"
    url="${url}&fromDate=${year}.${mm}.01T00:00&toDate=${next}T00:00"
    if curl -sf --max-time 120 -A "$agent" -o "$dest" "$url" &&
      grep -qv '^#' "$dest" 2>/dev/null; then
      files=$((files + 1))
    else
      # An answer with no data rows is the same outcome as no answer.
      rm -f "$dest"
      empty=$((empty + 1))
    fi
    sleep 1
  done
done < tools/giro-stations.tsv

cat > "${out}/fetched.txt" <<PROVENANCE
fetched: $(date -u +%Y-%m-%dT%H:%M:%SZ)
source: ${base} (GIRO / Lowell GIRO Data Center)
month: ${month}
files: ${files} present, ${empty} empty or missing, ${stations} stations asked
licence: CC-BY-NC-SA 4.0
attribution: Data supplied by the Global Ionospheric Radio Observatory
  (GIRO), Lowell GIRO Data Center. Follow the DIDBase rules of the road:
  https://ulcar.uml.edu/DIDBase/RulesOfTheRoadForDIDBase.htm
  Each station's data requires acknowledgement of its provider.
PROVENANCE

echo "${month}: ${stations} stations, ${files} files present, ${empty} empty or missing"
