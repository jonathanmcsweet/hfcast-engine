#!/usr/bin/env bash
#
# Fails if a change did not move the crate version.
#
#   tools/check-version-bump.sh <base-ref>       # e.g. origin/main
#
# One crate, one version, in Cargo.toml. The rule: a change to anything
# that ships in the published package has to move it, and a change to
# anything else — workflows, hooks, tools, documentation — does not.
#
# What ships is not written here. It is asked of cargo, from the
# `exclude` list in Cargo.toml, so this check and `cargo publish` can
# never disagree about where the boundary is. The package is listed at
# both ends of the comparison: a file added to the package is in the
# HEAD list, and a file deleted from it is in the base list.
#
# The comparison is "greater than", not "different from", so a version
# that goes backwards fails as well.
set -euo pipefail

base=${1:?usage: check-version-bump.sh <base-ref>}

cd "$(git rev-parse --show-toplevel)"

command -v cargo > /dev/null || {
  echo "cargo is needed: this check asks it what the package ships" >&2
  exit 1
}

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

# What ships at HEAD, from this checkout as it stands.
shipped_now="$(cargo package --list --allow-dirty 2> /dev/null)"

# What shipped at the base, from a temporary worktree, so a file this
# change deletes or newly excludes is still judged by the package it
# leaves.
shipped_before=""
if [[ -n "$(version_at "$base")" ]]; then
  worktree="$(mktemp -d)"
  trap 'git worktree remove --force "$worktree" 2> /dev/null || true' EXIT
  git worktree add --quiet --detach "$worktree" "$base"
  shipped_before="$(cd "$worktree" && cargo package --list 2> /dev/null || true)"
fi

# The changed files that are in either package listing. `comm` needs
# both sides sorted.
in_package="$(comm -12 \
  <(sort -u <<< "$changed") \
  <(printf '%s\n%s\n' "$shipped_now" "$shipped_before" | sort -u))"

if [[ -z $in_package ]]; then
  echo "nothing that ships changed against $base: the crate version holds"
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

echo "these files ship in the package:"
sed 's/^/  /' <<< "$in_package"
echo "::error file=Cargo.toml::the crate changed but its version did not move: still $before"
echo
echo "Move it in Cargo.toml, and run a build so Cargo.lock follows." >&2
exit 1
