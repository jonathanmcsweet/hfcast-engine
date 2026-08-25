#!/usr/bin/env bash
#
# Fetches a span of WSPR months, one bundle per month.
#
# `fetch-wspr.sh` answers for one month and writes it wherever it is
# told, which means a bare run overwrites the last one. Scoring a model
# across a solar cycle needs the months kept apart, so this writes each
# to `data/wspr-<YYYY-MM>/` and leaves them there.
#
# Resumable. A month whose bundle is already complete is skipped, so a
# span that stops halfway can be rerun without asking the service for
# anything it has already answered. Archive months do not change.
#
# The endpoint is a public research database and each month costs three
# aggregate queries over roughly 160 million rows, so the months go one
# at a time with a pause between them. About 30 seconds each, which puts
# a five-year span near half an hour.
#
# Usage: tools/fetch-wspr-span.sh 2021-01 2026-08
set -euo pipefail

from="${1:?usage: fetch-wspr-span.sh YYYY-MM YYYY-MM}"
to="${2:?usage: fetch-wspr-span.sh YYYY-MM YYYY-MM}"
pause="${WSPR_PAUSE:-5}"

cd "$(dirname "$0")/.."

months=()
cursor="$from-01"
while [ "$(date -u -d "$cursor" +%Y-%m)" \< "$to" ] || [ "$(date -u -d "$cursor" +%Y-%m)" = "$to" ]; do
  months+=("$(date -u -d "$cursor" +%Y-%m)")
  cursor="$(date -u -d "$cursor +1 month" +%Y-%m-%d)"
done

echo "${#months[@]} months, $from to $to"

done_count=0
skipped=0
failed=()
for m in "${months[@]}"; do
  out="data/wspr-$m"
  # A bundle is complete when both tables are there and the paths file
  # has rows under its header.
  if [ -s "$out/paths.csv" ] && [ -s "$out/hourly.csv" ] &&
     [ "$(wc -l < "$out/paths.csv")" -gt 1 ]; then
    skipped=$((skipped + 1))
    continue
  fi
  printf '%s ' "$m"
  # A month the archive does not cover answers with headers and no
  # rows, which is an ordinary outcome rather than a failure, but it is
  # not a bundle and must not be left looking like one.
  if tools/fetch-wspr.sh "$m" "$out" >/dev/null 2>&1 &&
     [ "$(wc -l < "$out/paths.csv")" -gt 1 ]; then
    echo "$(( $(wc -l < "$out/paths.csv") - 1 )) paths"
    done_count=$((done_count + 1))
  else
    echo "no data"
    rm -rf "$out"
    failed+=("$m")
  fi
  sleep "$pause"
done

echo
echo "fetched $done_count, already had $skipped, no data for ${#failed[@]}"
[ "${#failed[@]}" -gt 0 ] && echo "  ${failed[*]}"
exit 0
