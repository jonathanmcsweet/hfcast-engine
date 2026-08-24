#!/usr/bin/env bash
#
# Builds the two outside engines the comparison tools need.
#
#   tools/build-engines.sh           # build whatever is missing
#   tools/build-engines.sh --force   # build both again over the old ones
#
# `validate` and `engines` print our port beside two outside references:
# VOACAP, the Fortran program from ITS, and ITU-R P.533, the Study Group
# 3 reference program. `validate` checks for both before it runs any
# paths and stops with "both engines must be built" when either is
# missing, whichever engine the run was going to score. A `--ported` run
# still needs them, because P.533 is a column in every report.
#
# VOACAP already had two scripts, and this runs them in order:
# `build-itshfbc.sh` fetches and installs it and writes the data tree
# the port reads, and `build-variants.sh` builds the separate copy the
# harness runs. P.533 had no script, which is the gap this fills.
#
# Needs gfortran, gcc, make and git. About four minutes from nothing,
# nearly all of it VOACAP: P.533 compiles in under ten seconds, and its
# 700 MB is download rather than work.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
force=""
if [[ ${1:-} == --force ]]; then
  force=yes
fi

# Pinned for the reason `build-itshfbc.sh` pins voacapl: a change in a
# reference must not move a published comparison unless somebody chose
# it. This is the default branch on 2026-08-24, and it builds P533
# version 14.2.
#
# The repository also carries built binaries, and they are older than
# its own source: the committed `ITURHFProp` reports version 14.1 and a
# 2021 build date. So they are not used here.
ITU_REPO=https://github.com/ITU-R-Study-Group-3/ITU-R-HF
ITU_COMMIT=82017594a1c6cacfaa7e86954c4ae7b3a5825a3d
itu="$root/vendor/itu-r-hf"

# The two paths `src/itu.rs` and `src/runner.rs` look in. Checked at the
# end, so a layout that moves fails here rather than reporting success
# and leaving `validate` to refuse.
itu_bin="$itu/ITURHFProp/Linux/ITURHFProp"
voacap_bin="$root/vendor/voacapl-variants/O2/src/voacapw/voacapl"

for tool in gfortran gcc make git; do
  command -v "$tool" > /dev/null 2>&1 || {
    echo "build-engines: no $tool. Install it:" >&2
    echo "    sudo apt-get install -y gfortran gcc make git" >&2
    exit 1
  }
done

echo "== VOACAP: source, install and data tree"
if [[ -n $force ]]; then
  "$root/tools/build-itshfbc.sh" --force
else
  "$root/tools/build-itshfbc.sh"
fi

echo "== VOACAP: the binary the harness runs"
if [[ -n $force ]]; then
  rm -rf "$root/vendor/voacapl-variants/O2"
fi
# O2 alone. It is the variant every tool names, and the other four exist
# only to measure how far a listing moves under a different arithmetic
# evaluation order.
VARIANTS=O2 "$root/tools/build-variants.sh"

echo "== ITU-R P.533: source"
if [[ ! -d $itu/.git ]]; then
  # A shallow fetch of the pinned commit rather than a clone: the
  # working tree is 700 MB of coefficient files by itself and the
  # history is not wanted. `git fetch` needs the whole hash to name a
  # commit this way; a short one is refused as an unknown ref.
  rm -rf "$itu"
  mkdir -p "$itu"
  git -C "$itu" init --quiet
  git -C "$itu" remote add origin "$ITU_REPO"
  git -C "$itu" fetch --quiet --depth 1 origin "$ITU_COMMIT"
  git -C "$itu" checkout --quiet FETCH_HEAD
  echo "   fetched $ITU_COMMIT"
elif [[ $(git -C "$itu" rev-parse HEAD) != "$ITU_COMMIT" ]]; then
  # Moving the pin above moves the checkout, without the 700 MB again.
  git -C "$itu" fetch --quiet --depth 1 origin "$ITU_COMMIT"
  git -C "$itu" checkout --quiet FETCH_HEAD
  echo "   moved to $ITU_COMMIT"
else
  echo "   already at $ITU_COMMIT"
fi

echo "== ITU-R P.533: build"
# The checkout ships built binaries, so "the program is there" cannot
# mean "we built it": a fresh fetch already carries an `ITURHFProp`, and
# it is the 2021 one. A stamp written after a build of our own is what
# gets checked instead, named for the commit it was built from.
stamp="$itu/.built-$ITU_COMMIT"
if [[ -n $force ]]; then
  rm -f "$stamp"
fi
if [[ -f $stamp ]]; then
  echo "   already built"
else
  # One make for the three parts, which is what the repository's own
  # top-level Makefile is for, and what `engines` names when it cannot
  # find the program. `ITURHFProp` links neither library and opens both
  # at run time, which is why `src/itu.rs` sets LD_LIBRARY_PATH to the
  # two library directories rather than passing a path to the program.
  make -C "$itu/Linux" > "$itu/build.log" 2>&1 || {
    echo "build-engines: the P.533 build failed, see $itu/build.log" >&2
    exit 1
  }
  touch "$stamp"
  echo "   built from source"
fi

for bin in "$voacap_bin" "$itu_bin"; do
  [[ -x $bin ]] || {
    echo "build-engines: nothing built at $bin" >&2
    exit 1
  }
done

echo
echo "VOACAP      $voacap_bin"
echo "ITU-R P.533 $itu_bin"
LD_LIBRARY_PATH="$itu/P533/Linux:$itu/P372/Linux" "$itu_bin" -v |
  sed 's/^/            /'
