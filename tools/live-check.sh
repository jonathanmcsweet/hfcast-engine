#!/usr/bin/env bash
# Scores the engine against the current month's live ionosonde data.
#
# The current month is an ordinary month bundle (data/YYYY-MM) that is
# still filling in, so the whole validation machinery runs on it
# unchanged: the daily index fit, the storm conditioning, the full
# sonde report, the truecast API replay gate, and the absorption edge.
# One run does four things:
#
#   1. refreshes Kp and the month-to-date GIRO soundings (GIRO revises
#      recent scalings, so the live month is refetched, unlike archive
#      months which are fetched once);
#   2. fetches any IRTAM daily maps that have appeared (they lag the
#      present; missing days are ordinary);
#   3. writes the full report to data/live/report-<date>.txt and
#      appends one trend line to data/live/ledger.csv
#      (`sonde --ledger`: the most recent day, scored on its own rows);
#   4. replays the truecast point API against the research columns
#      (`sonde --engine truecast`) — the pass/fail gate. A nonzero exit
#      from this script means that gate failed or a fetch broke.
#
# The month needs a smoothed-SSN entry in src/wspr.rs (predicted for a
# live month; see the table's comment). See docs/soak.md.
#
# Usage: tools/live-check.sh  (no arguments; UTC decides the month)
set -euo pipefail

cd "$(dirname "$0")/.."
month=$(date -u +%Y-%m)
today=$(date -u +%Y-%m-%d)

tools/fetch-kp.sh
# Live scalings revise; refetch the month rather than trust the cache.
rm -rf "data/${month}/giro"
tools/fetch-giro.sh "${month}"
tools/fetch-irtam.sh "${month}" || echo "irtam: fetch incomplete (maps lag the present)"
rm -f "data/cache/${month}.sonde.csv"

mkdir -p data/live
report="data/live/report-${today}.txt"
cargo run --release --all-features --bin sonde -- \
  --kp data/kp_daily.txt "data/${month}" > "${report}"

ledger="data/live/ledger.csv"
line=$(cargo run --release --all-features --bin sonde -- \
  --ledger --kp data/kp_daily.txt "data/${month}" | tail -1)
if [ ! -s "${ledger}" ]; then
  echo "run,month,day,n_fof2,essn_bias,essn_mae,clim_bias,clim_mae,essn_index,n_fmin,edge_bias,edge_mae" > "${ledger}"
fi
echo "${today},${line}" >> "${ledger}"

echo "report: ${report}"
echo "ledger: ${line}"

cargo run --release --all-features --bin sonde -- \
  --engine truecast --kp data/kp_daily.txt "data/${month}"
