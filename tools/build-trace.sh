#!/usr/bin/env bash
#
# Builds the "trace" variant of voacapl: the vendored source with the
# instrumented files from trace/ copied over their originals, compiled with
# the reference flags (-O2). Each file in trace/ is a complete replacement
# for the same-named file under src/voacapw/, identical except for an added
# trace subroutine. The patched binary behaves identically unless the
# environment variable PROPCORE_TRACE names a directory, in which case each
# instrumented stage appends its intermediate values there. The Rust port
# is tested stage by stage against those values.
set -euo pipefail

SRC="${SRC:-/home/dev/workspace/vendor/voacapl}"
OUT="${OUT:-/home/dev/workspace/vendor/voacapl-variants}"
JOBS="${JOBS:-4}"

cd "$(dirname "$0")/.."
dir="$OUT/trace"

echo "== trace: copying source"
rm -rf "$dir"
cp -a "$SRC" "$dir"

echo "== trace: applying instrumented files"
for f in trace/*.for; do
  base="$(basename "$f")"
  if [ ! -f "$dir/src/voacapw/$base" ]; then
    echo "trace file $base has no counterpart in the source" >&2
    exit 1
  fi
  cp "$f" "$dir/src/voacapw/$base"
done

echo "== trace: building"
(
  cd "$dir"
  make distclean >distclean.log 2>&1 || true
  ./configure --prefix="$dir/prefix" \
    FCFLAGS="-g -O2" FFLAGS="-g -O2" >configure.log 2>&1
  make -j"$JOBS" >build.log 2>&1
)
echo "== trace: built $dir/src/voacapw/voacapl"
