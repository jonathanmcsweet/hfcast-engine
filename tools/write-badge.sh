#!/usr/bin/env bash
#
# Writes one shields.io endpoint badge.
#
#   tools/write-badge.sh <label> <job-status> <output-file>
#   tools/write-badge.sh --raw <label> <message> <colour> <output-file>
#
# The label is what the badge says on its left. The job status is
# GitHub's `job.status`: anything that is not "success" reads as a
# failure, so a cancelled or timed-out job shows red rather than
# disappearing.
#
# `--raw` states the message and the colour directly, for a badge that
# reports a measurement rather than a job. The soak badge uses it: that
# badge must say what the sweep found, not whether the job finished.
#
# shields.io reads the file from raw.githubusercontent and draws it. Its
# schema is at https://shields.io/badges/endpoint-badge
set -euo pipefail

if [[ ${1:-} == --raw ]]; then
  label=${2:?usage: write-badge.sh --raw <label> <message> <colour> <file>}
  message=${3:?usage: write-badge.sh --raw <label> <message> <colour> <file>}
  colour=${4:?usage: write-badge.sh --raw <label> <message> <colour> <file>}
  out=${5:?usage: write-badge.sh --raw <label> <message> <colour> <file>}
else
  label=${1:?usage: write-badge.sh <label> <status> <file>}
  status=${2:?usage: write-badge.sh <label> <status> <file>}
  out=${3:?usage: write-badge.sh <label> <status> <file>}

  if [[ $status == "success" ]]; then
    message=passing
    colour=brightgreen
  else
    message=failing
    colour=red
  fi
fi

mkdir -p "$(dirname "$out")"

printf '{"schemaVersion":1,"label":"%s","message":"%s","color":"%s"}\n' \
  "$label" "$message" "$colour" > "$out"

cat "$out"
