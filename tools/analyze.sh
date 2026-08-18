#!/usr/bin/env bash
#
# Static analysis, the way this crate needs it.
#
#   tools/analyze.sh          # run everything, print the findings
#   tools/analyze.sh --gate   # also fail if a gate is broken
#
# No single tool covers this for Rust. `rust-code-analysis` reports the
# per-function metrics but needs a host C++ toolchain to build its grammars,
# which not every machine has. So this runs several tools and says plainly
# which ones were not available.
#
# Two of the steps are gates and the rest are reports. The difference matters:
# most of `src/voacap/` is one Rust function per Fortran subroutine, and the
# measurements that make new code look bad are the ones that make a faithful
# port look bad too. A gate that fires on the port would be turned off within a
# week, so the gates only fire on change.
#
#   GATE   clippy, default lints
#   GATE   complexity, against tools/complexity-baseline.tsv
#   report public items nothing refers to
#   report clippy pedantic and nursery, minus the lints that would break parity
#   report duplication            (needs jscpd; skipped if absent)
#   report coverage               (needs cargo-llvm-cov; skipped if absent)
#   report size                   (needs tokei; skipped if absent)
#
# The optional tools, which are skipped rather than failed when absent:
#
#   cargo install cargo-llvm-cov tokei
#   rustup component add llvm-tools
#   npm install -g jscpd
#
# The complexity gate can also be run on its own:
#
#   cargo run --release --bin complexity             # the report
#   cargo run --release --bin complexity -- --check  # the gate
#   cargo run --release --bin complexity -- --update # rewrite the baseline
#
# Run --update only when a function is legitimately restructured, and say in
# the commit message why the new figure is right. Cyclomatic complexity counts
# branches, not difficulty: a dispatch over byte or token classes scores high
# and is not hard to read, which is why `complexity`'s own `walk` and `measure`
# are in the baseline.

set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

gate=0
[[ ${1:-} == --gate ]] && gate=1

out="target/analysis"
mkdir -p "$out"

failed=0
skipped=()

heading() {
  echo
  echo "=============================================================="
  echo "$1"
  echo "=============================================================="
}

# Lints that must never be applied here, and why.
#
# Casts and float comparisons are the Fortran's own; rewriting them would
# describe different arithmetic. `suboptimal_flops` and `imprecise_flops`
# suggest fused multiply-add and `ln_1p`, which round differently. That is
# exactly the class of change the parity harnesses exist to catch. The rest
# are documentation and naming preferences that say nothing about this code.
parity_allows=(
  -A clippy::cast_precision_loss
  -A clippy::cast_possible_truncation
  -A clippy::cast_sign_loss
  -A clippy::cast_possible_wrap
  -A clippy::cast_lossless
  -A clippy::float_cmp
  -A clippy::float_cmp_const
  -A clippy::suboptimal_flops
  -A clippy::imprecise_flops
  -A clippy::many_single_char_names
  -A clippy::similar_names
  -A clippy::unreadable_literal
  -A clippy::excessive_precision
  -A clippy::doc_markdown
  -A clippy::module_name_repetitions
  -A clippy::must_use_candidate
  -A clippy::missing_panics_doc
  -A clippy::missing_errors_doc
  -A clippy::missing_const_for_fn
)

# One codegen unit: the dev profile's default of 256 opens more files at link
# time than this host's descriptor limit allows.
export CARGO_PROFILE_DEV_CODEGEN_UNITS=1
export CARGO_PROFILE_TEST_CODEGEN_UNITS=1

heading "GATE  clippy, default lints"
# The status has to come from clippy, not from the pipeline. With `pipefail`
# a failing clippy sets the pipeline status no matter what `grep` found, so
# reading the pipeline would report every failure as a pass.
if cargo clippy --all-targets --message-format=short -- -D warnings \
  >"$out/clippy.txt" 2>&1; then
  echo "clean"
else
  grep -E 'warning:|error:' "$out/clippy.txt" | head -20
  echo "clippy is not clean"
  failed=1
fi

heading "GATE  complexity"
if ! cargo run -q --release --bin complexity -- --check 2>&1 | tee "$out/complexity-check.txt"; then
  failed=1
fi
cargo run -q --release --bin complexity >"$out/complexity.txt" 2>&1
echo "full report: $out/complexity.txt"

heading "public items nothing refers to"
# `dead_code` and `unreachable_pub` cannot see this: every `pub` item in a
# library is reachable by definition, so nothing warns about one that no
# caller ever names. A reference count does see it. One occurrence means the
# definition and nothing else.
: >"$out/deadcode.txt"
for name in $(grep -rhoE '^ *pub (fn|const|static) [A-Za-z_][A-Za-z0-9_]*' src/ |
  sed -E 's/^ *pub (fn|const|static) //' | sort -u); do
  count=$(grep -rwoh --include='*.rs' "$name" src tests | wc -l)
  if [[ $count -le 1 ]]; then
    echo "$name" >>"$out/deadcode.txt"
  fi
done
if [[ -s "$out/deadcode.txt" ]]; then
  echo "$(wc -l <"$out/deadcode.txt") item(s) with no reference outside their own definition:"
  sed 's/^/  /' "$out/deadcode.txt"
  echo "check each by hand: a name reached only through a macro reads the same way"
else
  echo "none"
fi

heading "clippy, pedantic and nursery, minus the parity lints"
cargo clippy --all-targets --message-format=json -- \
  -W clippy::pedantic -W clippy::nursery "${parity_allows[@]}" \
  >"$out/clippy-profile.json" 2>/dev/null
grep -oE '"code":"clippy::[a-z_]+"' "$out/clippy-profile.json" |
  sed 's/"code":"clippy:://;s/"//' | sort | uniq -c | sort -rn | head -25
echo
echo "counts are per compilation unit, so a lib finding is reported twice"
echo "full output: $out/clippy-profile.json"

heading "duplication"
if command -v jscpd >/dev/null 2>&1; then
  jscpd --min-tokens 60 --format rust --reporters console,json \
    --output "$out/cpd" src tests 2>&1 | tail -12
elif [[ -x node_modules/.bin/jscpd ]]; then
  node_modules/.bin/jscpd --min-tokens 60 --format rust --reporters console,json \
    --output "$out/cpd" src tests 2>&1 | tail -12
else
  echo "skipped: jscpd not installed (npm i -g jscpd)"
  skipped+=("duplication")
fi

heading "coverage"
if command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "unit and integration tests only; the parity harnesses are separate"
  echo "binaries and reach much more than this shows"
  cargo llvm-cov --lib --tests --summary-only 2>&1 | tee "$out/coverage.txt" | tail -25
else
  echo "skipped: cargo-llvm-cov not installed (cargo install cargo-llvm-cov)"
  skipped+=("coverage")
fi

heading "size"
if command -v tokei >/dev/null 2>&1; then
  tokei src tests
else
  echo "skipped: tokei not installed (cargo install tokei)"
  skipped+=("size")
fi

heading "summary"
if [[ ${#skipped[@]} -gt 0 ]]; then
  echo "not run: ${skipped[*]}"
fi
if [[ $failed -eq 1 ]]; then
  echo "a gate failed"
  [[ $gate -eq 1 ]] && exit 1
else
  echo "gates passed"
fi
exit 0
