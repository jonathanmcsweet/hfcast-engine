#!/usr/bin/env bash
#
# Puts badge files on the `badges` branch, which holds nothing else.
#
#   tools/publish-badges.sh <directory of .json files>
#
# The branch is an orphan: it shares no history with `main`, so it costs
# almost nothing and a reader who clones the repository does not get it
# unless they ask. shields.io reads each file from raw.githubusercontent
# and draws it.
#
# The branch is made on the first run. Later runs replace every file, so
# a badge for an architecture that no longer exists goes away with it.
set -euo pipefail

src=${1:?usage: publish-badges.sh <directory>}
src="$(cd "$src" && pwd)"

# Fail here rather than push an empty branch if the artefacts did not
# arrive.
if ! compgen -G "$src/*.json" > /dev/null; then
  echo "publish-badges.sh: no .json files in $src" >&2
  exit 1
fi

cd "$(git rev-parse --show-toplevel)"

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

work="$(mktemp -d)"
trap 'git worktree remove --force "$work" 2> /dev/null || true' EXIT

if git ls-remote --exit-code --heads origin badges > /dev/null 2>&1; then
  git fetch --quiet origin badges
  git worktree add --quiet -B badges "$work" FETCH_HEAD
else
  # No branch yet. An orphan checkout starts with the current tree in the
  # index, so empty it: the branch must hold the badges and nothing else.
  git worktree add --quiet --detach "$work"
  git -C "$work" checkout --quiet --orphan badges
  git -C "$work" rm --quiet -rf . > /dev/null 2>&1 || true
fi

find "$work" -maxdepth 1 -name '*.json' -delete
cp "$src"/*.json "$work/"

git -C "$work" add -A

if git -C "$work" diff --cached --quiet; then
  echo "the badges did not change"
  exit 0
fi

git -C "$work" commit --quiet -m "ci: update the architecture badges"
git -C "$work" push --quiet origin badges
echo "pushed $(find "$src" -maxdepth 1 -name '*.json' | wc -l) badges"
