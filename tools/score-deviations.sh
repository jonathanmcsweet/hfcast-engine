#!/usr/bin/env bash
#
# Scores every WSPR bundle once per deviation, one deviation at a time.
# Run from the engine repo root, after `cargo build --release
# --all-features`. Resumable: a report that is already there is skipped.
set -uo pipefail
BIN=${BIN:-./target/release/validate}
out=${OUT:-dumps/deviation-scoring}
mkdir -p "$out"
for d in data/wspr-*/; do
  m=$(basename "$d")
  for v in reference fast-cos fast-cos-modes exact-height all; do
    f="$out/$m-$v.md"
    [ -s "$f" ] && continue
    case $v in
      reference) flags=() ;;
      all)       flags=(--truecast-numerics) ;;
      *)         flags=(--numerics "$v") ;;
    esac
    echo "== $m $v"
    "$BIN" --ported "${flags[@]}" --data "$d" > "$f" 2> "${f%.md}.log"
  done
done
echo "scored $(ls "$out"/*.md | wc -l) reports"
