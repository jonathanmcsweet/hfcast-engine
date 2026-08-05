#!/usr/bin/env bash
#
# Builds `voacapl` and writes the `itshfbc` data tree the tests read.
#
#   tools/build-itshfbc.sh           # build it if it is not there
#   tools/build-itshfbc.sh --force   # build it again over the old one
#   tools/build-itshfbc.sh --commit  # print the voacapl commit and stop
#
# Eight tests open antenna files under `<itshfbc>/antennas`, and two more
# copy a tree to give a run its own. Without the tree all ten fail with a
# "No such file or directory" panic, which names the file and not the
# reason. The tree comes from `makeitshfbc`, which is part of voacapl, so
# building the reference is how you get it.
#
# The engine looks in `$HFCAST_ITSHFBC`, and in `~/itshfbc` when that is
# not set. `makeitshfbc` writes to `~/itshfbc`, so the two agree with no
# configuration.
#
# Needs gfortran, make and a C compiler. About three minutes on a runner.
set -euo pipefail

# The reference is pinned so that a change in voacapl cannot move this
# engine's answers without somebody choosing it. CI reads the same value
# from here with `--commit`, so there is one copy of it.
VOACAPL_COMMIT=c12a98b

if [[ ${1:-} == --commit ]]; then
  echo "$VOACAPL_COMMIT"
  exit 0
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tree="${HFCAST_ITSHFBC:-$HOME/itshfbc}"

# `makeitshfbc` does not copy the data. It builds a tree of symlinks into
# the voacapl install, so `$tree/coeffs`, `geocity`, `geonatio`, `geostate`
# and four files under `database/` point at
# `~/.local/share/voacapl/itshfbc/`. The tree is a set of directions to
# the data, not the data.
#
# So "the directory is there" answers the wrong question. Move or drop the
# install and every link dangles while the directory still looks fine, and
# the tests fail with "No such file or directory" naming a file that is
# right in front of you. Read something from behind a symlink instead: it
# is the coefficients that matter and the coefficients that go missing.
if [[ -r $tree/coeffs/coeff01.bin && ${1:-} != --force ]]; then
  echo "itshfbc tree already at $tree"
  exit 0
fi

if [[ -d $tree && ${1:-} != --force ]]; then
  echo "build-itshfbc: $tree exists but its data is unreachable." >&2
  echo "               Its symlinks point into a voacapl install that is" >&2
  echo "               not there. Rebuilding both." >&2
fi

for tool in gfortran make; do
  command -v "$tool" > /dev/null 2>&1 || {
    echo "build-itshfbc: no $tool. Install it:" >&2
    echo "    sudo apt-get install -y gfortran make" >&2
    exit 1
  }
done

src="$root/vendor/voacapl"
rm -rf "$src"
mkdir -p "$root/vendor"
git clone --quiet https://github.com/jawatson/voacapl.git "$src"
git -C "$src" checkout --quiet "$VOACAPL_COMMIT"
cd "$src"

# git does not record modification times, so a fresh clone can leave
# `aclocal.m4` looking older than `configure.ac`. make then tries to run
# aclocal-1.15 to make it again, which is not installed and is not
# needed: the generated files are in the repository. Setting the inputs
# old and the outputs new stops make trying. Without this the build stops
# at "aclocal-1.15: command not found".
find . \( -name 'configure.ac' -o -name 'configure.in' \
  -o -name 'Makefile.am' -o -name 'acinclude.m4' \) \
  -exec touch -d '2000-01-01' {} +
find . \( -name 'aclocal.m4' -o -name 'configure' \
  -o -name 'Makefile.in' -o -name 'config.h.in' \) -exec touch {} +

./configure --prefix="$HOME/.local"

# Serial. `itshfbc/bin/anttyp99` compiles a module that two other files
# read, and the generated Makefile does not declare that dependency, so a
# parallel make can start a reader first and fail on a missing
# `cant99.mod`.
make
make install

# The installed copy: the prefix is substituted into it by the install
# hook, and the source copy still says __PREFIX__.
"$HOME/.local/bin/makeitshfbc"

[[ -d $tree ]] || {
  echo "build-itshfbc: makeitshfbc wrote no tree at $tree" >&2
  exit 1
}
echo "itshfbc tree at $tree"
