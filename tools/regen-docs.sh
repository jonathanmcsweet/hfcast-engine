#!/usr/bin/env bash
#
# Regenerates the documents that are printouts of this repository's own
# tools, and checks that the committed copies still say what the tools
# say now.
#
#   tools/regen-docs.sh           # rewrite any document whose numbers moved
#   tools/regen-docs.sh --check   # report and fail instead of rewriting
#
# Six documents under docs/ are not written by hand. Each one is what a
# tool printed on the day somebody ran it, and until this script existed
# nothing ever ran them again. docs/ionosonde-output.md drifted behind
# its own inputs that way: GIRO re-scaled its archive, the local cache
# picked the new scaling up, and the document did not.
#
# Two of the six, docs/sensitivity.md and docs/reliability.md, are
# printouts with prose added on top by hand. Only their tables are
# checked, and they are never rewritten.
#
# Column padding is ignored. The tools print markdown tables with one
# space between the bars, and the committed copies are padded so that
# the columns line up. That padding is not a claim about radio, so a
# document whose numbers all match is treated as current and is left
# alone with its formatting intact.
#
# Every document needs measured data that the repository does not carry,
# because the data is large and most of it is not ours to redistribute.
# A run without that data skips the document and names it rather than
# failing, which is the same rule tools/analyze.sh follows. `--check`
# fails only on a document it could actually rebuild.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

check=0
[[ ${1:-} == --check ]] && check=1

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

rebuilt=()
moved=()
skipped=()

# The eight months every ionosonde-based document is measured over, and
# the six the calibration matrix fits across. Both lists are the ones
# the committed documents were built from; changing either changes what
# the document claims, so they are written here rather than discovered
# from whatever happens to be on disk.
SONDE_MONTHS=(2015-03 2019-06 2019-12 2022-09 2024-12 2025-03 2025-06 2025-07)
STORM_FIT=2025-06
STORM_TEST=(2015-03 2022-09 2024-12 2025-03 2025-07 2019-06 2019-12)

# Where a month's WSPR reception reports sit. The fetch scripts put
# them in data/<month>, beside that month's ionosonde data, which is the
# layout every document describes and the one the tools expect. A
# machine that fetched the reports before that settled has them in
# data/wspr-<month> instead, so both are accepted and the merged one
# wins. Prints nothing when the month has neither.
wspr_dir() {
  if [[ -f data/$1/month.txt ]]; then
    echo "data/$1"
  elif [[ -f data/wspr-$1/month.txt ]]; then
    echo "data/wspr-$1"
  fi
}

# A markdown table with its padding taken out, so that two copies of the
# same numbers compare equal however their columns are spaced. Only
# lines that are entirely a table row are touched, so a bar inside prose
# is left alone.
normalise() {
  sed -e '/^[[:space:]]*|.*|[[:space:]]*$/{
            s/[[:space:]]*|[[:space:]]*/|/g
            s/^|//
            s/|$//
            /^[-:|]*$/s/--*/-/g
          }' "$1"
}

# Just the table rows of a document, normalised. A document that is a
# printout with hand-written prose on top can only be checked this far:
# the prose is the maintainer's and rebuilding the file would throw it
# away, so the figures are compared and the words are left alone.
tables() {
  grep -E '^[[:space:]]*\|.*\|[[:space:]]*$' "$1" |
    sed -e 's/[[:space:]]*|[[:space:]]*/|/g' \
      -e 's/^|//' \
      -e 's/|$//' \
      -e '/^[-:|]*$/s/--*/-/g'
}

# Checks a hybrid document's figures without touching it. It is never
# rewritten, in either mode, because its prose is not ours to replace.
#
# These documents open with the tool's output and continue with sections
# somebody wrote afterwards, so the rule is that the tool's table rows
# are the document's first table rows. Anything the document adds below
# them is its own and is not compared.
settle_tables() {
  local name=$1 out=$2 new=$3
  tables "$new" > "$tmp/want.tsv"
  tables "$out" | head -n "$(wc -l < "$tmp/want.tsv")" > "$tmp/have.tsv"
  if diff -q "$tmp/want.tsv" "$tmp/have.tsv" > /dev/null; then
    rebuilt+=("$name (tables)")
    return
  fi
  moved+=("$name")
  echo
  echo "$out has moved, in its tables:"
  diff -u "$tmp/have.tsv" "$tmp/want.tsv" | sed -e '1,2d' -e 's/^/  /' | head -40
  echo "  this document has hand-written prose, so it is not rewritten."
}

# Compares one freshly built document against the committed copy, and
# either installs it or reports it, depending on the mode.
settle() {
  local name=$1 out=$2 new=$3
  if [[ ! -f $out ]]; then
    moved+=("$name (no committed copy)")
    [[ $check -eq 0 ]] && cp "$new" "$out" && echo "wrote $out"
    return
  fi
  if normalise "$out" | diff -q - <(normalise "$new") > /dev/null; then
    rebuilt+=("$name")
    return
  fi
  moved+=("$name")
  echo
  echo "$out has moved:"
  diff -u <(normalise "$out") <(normalise "$new") |
    sed -e '1,2d' -e 's/^/  /' | head -40
  if [[ $check -eq 0 ]]; then
    cp "$new" "$out"
    echo "  rewrote $out"
  fi
}

# ---- docs/ionosonde-output.md ---------------------------------------
#
# `sonde` over the eight month bundles. It reads its own cache when it
# finds one, which is why the cache has to round-trip exactly: a cold
# run and a warm run that disagree would make this document impossible
# to check. See `save_cache` in src/sonde.rs.
missing=()
for m in "${SONDE_MONTHS[@]}"; do
  [[ -d data/$m ]] || missing+=("data/$m")
done
[[ -f data/kp_daily.txt ]] || missing+=("data/kp_daily.txt")
if [[ ${#missing[@]} -eq 0 ]]; then
  months=("${SONDE_MONTHS[@]/#/data/}")
  if cargo run --release --quiet --all-features --bin sonde -- \
    --kp data/kp_daily.txt "${months[@]}" > "$tmp/ionosonde-output.md" 2> "$tmp/sonde.err"; then
    settle ionosonde-output.md docs/ionosonde-output.md "$tmp/ionosonde-output.md"
  else
    echo "sonde failed:"
    sed 's/^/  /' "$tmp/sonde.err" | head -10
    moved+=("ionosonde-output.md (sonde failed)")
  fi
else
  skipped+=("ionosonde-output.md: needs ${missing[*]}")
fi

# ---- docs/storm-output.md -------------------------------------------
#
# `storm` over the WSPR bundles, not the ionosonde ones: it asks how
# wide the spread has to be on a disturbed day, and the spread is
# measured against reception reports.
missing=()
fit_dir="$(wspr_dir "$STORM_FIT")"
[[ -n $fit_dir ]] || missing+=("the WSPR reports for $STORM_FIT")
test_dirs=()
for m in "${STORM_TEST[@]}"; do
  d="$(wspr_dir "$m")"
  if [[ -n $d ]]; then
    test_dirs+=("$d")
  else
    missing+=("the WSPR reports for $m")
  fi
done
[[ -f data/kp_daily.txt ]] || missing+=("data/kp_daily.txt")
if [[ ${#missing[@]} -eq 0 ]]; then
  args=(--kp data/kp_daily.txt --cache data/cache --fit "$fit_dir")
  for d in "${test_dirs[@]}"; do
    args+=(--test "$d")
  done
  if cargo run --release --quiet --bin storm -- "${args[@]}" \
    > "$tmp/storm-output.md" 2> "$tmp/storm.err"; then
    settle storm-output.md docs/storm-output.md "$tmp/storm-output.md"
  else
    echo "storm failed:"
    sed 's/^/  /' "$tmp/storm.err" | head -10
    moved+=("storm-output.md (storm failed)")
  fi
else
  skipped+=("storm-output.md: needs ${missing[*]}")
fi

# ---- docs/calibration-matrix.md and -es.md --------------------------
#
# Both come from the same script over the same months, once with the
# standard dump and once with the one VOACAP's sporadic-E term is on
# for. The script writes straight to its output path, so it builds into
# the temporary directory and `settle` decides what happens next.
for pair in "docs/calibration-matrix.md hours.csv" "docs/calibration-matrix-es.md hours-es.csv"; do
  set -- $pair
  out=$1 dump=$2
  name=${out#docs/}
  missing=()
  for m in 2025-06 2025-07 2025-03 2024-12 2019-06 2019-12; do
    [[ -f data/$m/$dump ]] || missing+=("data/$m/$dump")
  done
  if [[ ${#missing[@]} -eq 0 ]]; then
    if tools/calibration-matrix.sh "$tmp/$name" "$dump" > "$tmp/cal.err" 2>&1; then
      settle "$name" "$out" "$tmp/$name"
    else
      echo "calibration-matrix.sh failed:"
      sed 's/^/  /' "$tmp/cal.err" | head -10
      moved+=("$name (calibration-matrix.sh failed)")
    fi
  else
    skipped+=("$name: needs ${missing[*]} — run validate --dump first")
  fi
done

# ---- docs/sensitivity.md --------------------------------------------
#
# `measure` runs the same 96 cases through the reference Fortran built
# five ways, so that what is left is the model's sensitivity to
# evaluation order. The envelope it prints is what `portcheck` judges
# the port against, which makes it the most load-bearing of these
# documents. It needs all five builds, so a checkout with only the O2
# reference skips it.
#
# Its tables alone are compared: the committed copy carries a title and
# a paragraph somebody wrote by hand, and one timing that is a property
# of the machine rather than of the model.
missing=()
for v in O0 O1 O2 O3 fastmath; do
  [[ -x vendor/voacapl-variants/$v/src/voacapw/voacapl ]] || missing+=("$v")
done
if [[ ${#missing[@]} -eq 0 ]]; then
  if cargo run --release --quiet --bin measure > "$tmp/sensitivity.md" 2> "$tmp/measure.err"; then
    settle_tables sensitivity.md docs/sensitivity.md "$tmp/sensitivity.md"
  else
    echo "measure failed:"
    sed 's/^/  /' "$tmp/measure.err" | head -10
    moved+=("sensitivity.md (measure failed)")
  fi
else
  skipped+=("sensitivity.md: needs the ${missing[*]} reference builds — run VARIANTS='${missing[*]}' tools/build-variants.sh")
fi

# ---- docs/reliability.md --------------------------------------------
#
# `reliability` asks whether the day-to-day spread the app turns into a
# "chance of rain" is honest, one month fitted and five tested. The
# document opens with that report and continues with the storm analysis,
# which is written by hand over docs/storm-output.md.
#
# The month order is the order the committed document was built in and
# is part of what it says, since each section is headed by its month.
RELIABILITY_FIT=2025-06
RELIABILITY_TEST=(2025-07 2022-09 2024-12 2019-12 2015-03)
missing=()
fit_dir="$(wspr_dir "$RELIABILITY_FIT")"
[[ -n $fit_dir ]] || missing+=("the WSPR reports for $RELIABILITY_FIT")
test_dirs=()
for m in "${RELIABILITY_TEST[@]}"; do
  d="$(wspr_dir "$m")"
  if [[ -n $d ]]; then
    test_dirs+=("$d")
  else
    missing+=("the WSPR reports for $m")
  fi
done
if [[ ${#missing[@]} -eq 0 ]]; then
  args=(--fit "$fit_dir")
  for d in "${test_dirs[@]}"; do
    args+=(--test "$d")
  done
  if cargo run --release --quiet --bin reliability -- "${args[@]}" \
    > "$tmp/reliability.md" 2> "$tmp/reliability.err"; then
    settle_tables reliability.md docs/reliability.md "$tmp/reliability.md"
  else
    echo "reliability failed:"
    sed 's/^/  /' "$tmp/reliability.err" | head -10
    moved+=("reliability.md (reliability failed)")
  fi
else
  skipped+=("reliability.md: needs ${missing[*]}")
fi

# ---- summary --------------------------------------------------------
echo
if [[ ${#rebuilt[@]} -gt 0 ]]; then
  echo "current: ${rebuilt[*]}"
fi
if [[ ${#skipped[@]} -gt 0 ]]; then
  echo "not run:"
  printf '  %s\n' "${skipped[@]}"
fi
if [[ ${#moved[@]} -gt 0 ]]; then
  echo "moved: ${moved[*]}"
  if [[ $check -eq 1 ]]; then
    echo "::error::a generated document no longer matches its tool; run tools/regen-docs.sh"
    exit 1
  fi
  exit 0
fi
echo "every generated document that could be rebuilt matches its tool."
exit 0
