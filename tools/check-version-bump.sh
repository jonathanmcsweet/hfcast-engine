#!/usr/bin/env bash
#
# Fails if a change did not move the crate version.
#
#   tools/check-version-bump.sh <base-ref>       # e.g. origin/main
#
# One crate, one version, in Cargo.toml. Any change to the repository has
# to move it.
#
# The comparison is "greater than", not "different from", so a version
# that goes backwards fails as well.
set -euo pipefail

base=${1:?usage: check-version-bump.sh <base-ref>}

cd "$(git rev-parse --show-toplevel)"

# The first `version = "..."` line of Cargo.toml, which is the package's
# own. Dependency versions come later and this crate has none.
version_at() {
  git show "$1:Cargo.toml" 2> /dev/null | grep -m1 '^version = ' | sed 's/.*"\(.*\)".*/\1/' || true
}

is_later() {
  [[ $1 != "$2" ]] && [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" == "$1" ]]
}

changed="$(git diff --name-only "$base"...HEAD)"

if [[ -z $changed ]]; then
  echo "no files changed against $base"
  exit 0
fi

before="$(version_at "$base")"
after="$(version_at HEAD)"

if [[ -z $before ]]; then
  echo "Cargo.toml is new, nothing to compare"
  exit 0
fi

if is_later "$after" "$before"; then
  echo "the crate: $before -> $after"
  exit 0
fi

echo "::error file=Cargo.toml::the crate changed but its version did not move: still $before"
echo
echo "Move it in Cargo.toml, and run a build so Cargo.lock follows." >&2
exit 1
