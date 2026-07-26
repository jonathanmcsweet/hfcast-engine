#!/usr/bin/env bash
#
# Builds voacapl several times with different Fortran optimisation flags.
#
# The point is measurement, not packaging. Each variant computes the same model
# with a different arithmetic evaluation order, so comparing their outputs shows
# how much a listing moves for reasons that carry no physical meaning. That
# spread is the floor for any port tolerance.
#
# The vendored tree is already configured in place, and autotools refuses a
# separate build directory in that situation. Each variant therefore gets its
# own copy of the source, which also leaves the vendored tree and the installed
# binary at ~/.local/bin/voacapl untouched. A copy is about 12 MB.
set -euo pipefail

SRC="${SRC:-/home/dev/workspace/vendor/voacapl}"
OUT="${OUT:-/home/dev/workspace/vendor/voacapl-variants}"

# The host has 16 cores but under 3 GB of usable RAM. Sizing the job count from
# core count gets the compiler OOM-killed, which surfaces as a bare "Killed".
JOBS="${JOBS:-4}"

# -O2 matches how the vendored binary is built and is the reference.
# -ffast-math is included deliberately as an out-of-contract case: it permits
# reassociation and drops strict IEEE semantics, so it shows what a port that
# rearranges arithmetic "harmlessly" would cost.
VARIANT_NAMES=(O0 O1 O2 O3 fastmath)
variant_flags() {
  case "$1" in
    O0) echo "-g -O0" ;;
    O1) echo "-g -O1" ;;
    O2) echo "-g -O2" ;;
    O3) echo "-g -O3" ;;
    fastmath) echo "-g -O2 -ffast-math" ;;
    *) return 1 ;;
  esac
}

if [ ! -x "$SRC/configure" ]; then
  echo "no configure script at $SRC — is the voacapl source present?" >&2
  exit 1
fi

mkdir -p "$OUT"
failed=()

for name in "${VARIANT_NAMES[@]}"; do
  flags="$(variant_flags "$name")"
  dir="$OUT/$name"

  if [ -x "$dir/src/voacapw/voacapl" ]; then
    echo "== $name: already built, skipping"
    continue
  fi

  echo "== $name: copying source"
  rm -rf "$dir"
  cp -a "$SRC" "$dir"

  echo "== $name: building with FCFLAGS='$flags'"
  if (
    cd "$dir"
    # The copy inherits the vendored tree's configuration and object files.
    make distclean >distclean.log 2>&1 || true
    ./configure --prefix="$dir/prefix" \
      FCFLAGS="$flags" FFLAGS="$flags" >configure.log 2>&1
    make -j"$JOBS" >build.log 2>&1
  ); then
    echo "== $name: built $dir/src/voacapw/voacapl"
  else
    echo "== $name: FAILED — see $dir/configure.log and $dir/build.log" >&2
    failed+=("$name")
  fi
done

if [ ${#failed[@]} -gt 0 ]; then
  echo "failed variants: ${failed[*]}" >&2
  exit 1
fi

echo "all variants present in $OUT"
