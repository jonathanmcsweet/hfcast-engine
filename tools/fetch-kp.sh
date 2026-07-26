#!/usr/bin/env bash
# Downloads the GFZ Potsdam daily geomagnetic index record (Kp, ap, Ap) into
# data/kp_daily.txt. One file covers 1932 to the present, so the storm
# analysis needs no per-month fetching. See src/geomag.rs for the format.
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p data

curl -sfL --max-time 300 \
  -o data/kp_daily.txt \
  "https://kp.gfz.de/app/files/Kp_ap_Ap_SN_F107_since_1932.txt"

lines=$(wc -l < data/kp_daily.txt)
echo "data/kp_daily.txt: ${lines} lines"
