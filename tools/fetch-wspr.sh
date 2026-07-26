#!/usr/bin/env bash
#
# Downloads a month of WSPR reception reports, already aggregated.
#
# WSPR is the only large public source that records both what was transmitted
# and what was received: every report carries the transmit power, the measured
# signal-to-noise ratio, both grid locations and a timestamp. That is exactly
# the input and output pair a propagation model claims to predict.
#
# The archive for one month is around 160 million reports, so the aggregation
# is done by the server and only the summary is downloaded. Two files come out:
#
#   paths.csv   one row per transmitter, receiver and band, with the geometry
#   hourly.csv  one row per path and UTC hour, with the median reported SNR
#
# A path here is a fixed pair of stations on a fixed band. Holding the stations
# fixed holds their antennas and their local noise fixed too, which is what
# makes the comparison possible at all: those are unknown, but within one path
# they are constant, so they become one offset rather than noise.
set -euo pipefail

MONTH="${1:-2025-06}"
OUT="${2:-$(dirname "$0")/../data}"

# The public WSPR database exposes a read-only SQL endpoint.
ENDPOINT="${WSPR_ENDPOINT:-https://db1.wspr.live/}"

# Below this a path is short enough for ground wave to matter, and the models
# are being asked about skywave. Above it the great-circle assumption weakens.
MIN_KM="${MIN_KM:-800}"
MAX_KM="${MAX_KM:-12000}"

# Bands that map onto the amateur allocations the app predicts.
BANDS="3,7,10,14,18,21,24,28"

# Enough reports for the hourly medians to mean something.
MIN_REPORTS="${MIN_REPORTS:-400}"

# How many distinct UTC hours a path must appear in.
#
# Not 24. Demanding a path be present in every hour selects circuits that never
# close, and those barely vary through the day — the one regime where a model
# has nothing to add over quoting the average. Predicting when a band opens and
# closes is the interesting part, so paths that go quiet for part of the day
# have to be in the sample.
MIN_HOURS_COVERED="${MIN_HOURS_COVERED:-12}"

# One transmitter can dominate a month; WW0WWV alone accounts for many of the
# best-covered paths. Capping keeps the sample from becoming a test of one
# station's antenna.
MAX_PER_TX="${MAX_PER_TX:-2}"
MAX_PATHS="${MAX_PATHS:-150}"

start="${MONTH}-01"
end="$(date -u -d "${start} +1 month" +%Y-%m-%d)"

mkdir -p "$OUT"

query() {
  curl -sS --fail --max-time 300 -G "$ENDPOINT" \
    --data-urlencode "query=$1 FORMAT CSVWithNames"
}

# Common row filter, repeated rather than shared so each query stands alone.
WHERE="time >= '${start}' AND time < '${end}'
  AND distance >= ${MIN_KM} AND distance <= ${MAX_KM}
  AND band IN (${BANDS})"

# Stage one: which paths are worth using.
#
# Deliberately counts and nothing else. Adding the medians here makes the
# server compute them for every one of the millions of station pairs in a
# month, which takes long enough that the request times out. Counting is
# seconds; the medians come later, for eighty paths rather than millions.
CHOSEN="
SELECT tx_sign, rx_sign, band
FROM wspr.rx
WHERE ${WHERE}
GROUP BY tx_sign, rx_sign, band
HAVING uniq(toHour(time)) >= ${MIN_HOURS_COVERED} AND count() >= ${MIN_REPORTS}
ORDER BY count() DESC
LIMIT ${MAX_PER_TX} BY tx_sign
LIMIT ${MAX_PATHS}"

# Stage two: geometry for those paths only.
#
# Medians rather than any() for the positions: a station that corrected its
# locator mid-month would otherwise contribute a place it never transmitted
# from.
echo "fetching paths for ${MONTH}" >&2
query "
WITH chosen AS (${CHOSEN})
SELECT tx_sign, rx_sign, band,
       count() AS reports,
       round(median(distance)) AS km,
       round(median(power)) AS power_dbm,
       round(median(tx_lat), 4) AS tx_lat,
       round(median(tx_lon), 4) AS tx_lon,
       round(median(rx_lat), 4) AS rx_lat,
       round(median(rx_lon), 4) AS rx_lon,
       round(median(frequency)) AS freq_hz
FROM wspr.rx
WHERE ${WHERE}
  AND (tx_sign, rx_sign, band) IN (SELECT tx_sign, rx_sign, band FROM chosen)
GROUP BY tx_sign, rx_sign, band
ORDER BY reports DESC" > "$OUT/paths.csv"
echo "  $(( $(wc -l < "$OUT/paths.csv") - 1 )) paths" >&2

# Stage three: hourly medians for those paths only.
echo "fetching hourly medians" >&2
query "
WITH chosen AS (${CHOSEN})
SELECT tx_sign, rx_sign, band,
       toHour(time) AS hour,
       count() AS reports,
       round(median(snr), 2) AS snr_median
FROM wspr.rx
WHERE ${WHERE}
  AND (tx_sign, rx_sign, band) IN (SELECT tx_sign, rx_sign, band FROM chosen)
GROUP BY tx_sign, rx_sign, band, hour
ORDER BY tx_sign, rx_sign, band, hour" > "$OUT/hourly.csv"
echo "  $(( $(wc -l < "$OUT/hourly.csv") - 1 )) path-hours" >&2

echo "$MONTH" > "$OUT/month.txt"
echo "wrote $OUT/paths.csv, $OUT/hourly.csv, $OUT/month.txt" >&2
