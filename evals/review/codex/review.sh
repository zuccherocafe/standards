#!/bin/bash
# Reviews the packets review/blind.sh made, with Harbor's Codex agent on a Codex login, REVIEW_ANALYSES_AT_ONCE
# analyses at a time (1 to 9, and 3 when unset or empty) and each packet twice, joins the verdicts with the key, and
# scans everything the review wrote for the login before printing any of it. Run it from evals/ inside the development
# shell, after building the image as evals/README.md says. It takes no other argument, so nothing can move Harbor's
# output, its configuration or the agent's environment:
#
#   CODEX_AUTH_JSON_PATH=<login file> REVIEW_LOGIN_RISK=accepted review/codex/review.sh <review folder>
#   REVIEW_REAL_LOGIN=1 REVIEW_LOGIN_RISK=accepted review/codex/review.sh <review folder>
#
# The first names the login file, the second takes ~/.codex/auth.json. REVIEW_LOGIN_RISK=accepted acknowledges what
# evals/README.md says the scan cannot catch. Harbor's output goes to <review folder>/harbor.log and the join's to
# <review folder>/results.txt, inside what the scan reads; a copy of the login, made once and used for the upload,
# Harbor's scrubber and the scan alike, stays in a private folder outside it and is removed at the end.
set -euo pipefail

stop() {
    echo "review: $*" >&2
    exit 1
}

[ "$#" -eq 1 ] || stop "usage: review/codex/review.sh <review folder>"
# Every analysis at once runs its own container on the one login, so the count stays small.
at_once=${REVIEW_ANALYSES_AT_ONCE:-3}
[[ $at_once =~ ^[1-9]$ ]] || stop "REVIEW_ANALYSES_AT_ONCE must be a whole number from 1 to 9, not '$at_once'"
# The folder by its real path, so that what Harbor writes and what the scan reads are one folder.
review=$(realpath -e -- "$1") || stop "$1 does not exist"
if [ -n "${CODEX_AUTH_JSON_PATH:-}" ] && [ "${REVIEW_REAL_LOGIN:-}" = 1 ]; then
    stop "choose one login: CODEX_AUTH_JSON_PATH or REVIEW_REAL_LOGIN=1, not both"
elif [ -n "${CODEX_AUTH_JSON_PATH:-}" ]; then
    login=$CODEX_AUTH_JSON_PATH
elif [ "${REVIEW_REAL_LOGIN:-}" = 1 ]; then
    login=$HOME/.codex/auth.json
else
    stop "choose the login: CODEX_AUTH_JSON_PATH=<file>, or REVIEW_REAL_LOGIN=1 for ~/.codex/auth.json"
fi
[ "${REVIEW_LOGIN_RISK:-}" = accepted ] \
    || stop "set REVIEW_LOGIN_RISK=accepted once you have read what evals/README.md says the scan cannot catch"
[ -f "$review/key.json" ] && [ -d "$review/blind" ] || stop "$review holds no packets from review/blind.sh"
[ ! -e "$review/review" ] || stop "$review already holds a review; make the packets again in a new folder"
[ -r "$login" ] || stop "cannot read the login $login"

private=$(mktemp -d)
chmod 700 "$private"
harbor_pid=""
# shellcheck disable=SC2329 # the EXIT trap below runs it
finish() {
    if [ -n "$harbor_pid" ] && kill -0 "$harbor_pid" 2> /dev/null; then
        kill -INT "$harbor_pid" 2> /dev/null || true
        wait "$harbor_pid" 2> /dev/null || true
    fi
    rm -rf "$private"
}
trap finish EXIT
interrupted=0
trap 'interrupted=1' INT TERM

cp "$login" "$private/auth.json"
export CODEX_AUTH_JSON_PATH=$private/auth.json
CODEX_LOGIN_ACCESS_TOKEN=$(jq -er '.tokens.access_token' "$private/auth.json") || stop "the login has no access token"
CODEX_LOGIN_REFRESH_TOKEN=$(jq -er '.tokens.refresh_token' "$private/auth.json") \
    || stop "the login has no refresh token"
CODEX_LOGIN_ID_TOKEN=$(jq -er '.tokens.id_token' "$private/auth.json") || stop "the login has no ID token"
export CODEX_LOGIN_ACCESS_TOKEN CODEX_LOGIN_REFRESH_TOKEN CODEX_LOGIN_ID_TOKEN

# The single quotes keep each value a template, which Harbor fills from the environment and records unfilled; Harbor
# treats the names as secrets and replaces their values with [REDACTED] in each trial's files.
# shellcheck disable=SC2016
harbor analyze "$review/blind" -r review/rubric.toml -p review/prompt.txt --task-config review/codex/task.toml \
    -e podman -n "$at_once" -k 2 -a codex -m gpt-6-astra --ak version=0.163.0 --ak reasoning_effort=high \
    --ae 'CODEX_LOGIN_ACCESS_TOKEN=${CODEX_LOGIN_ACCESS_TOKEN}' \
    --ae 'CODEX_LOGIN_REFRESH_TOKEN=${CODEX_LOGIN_REFRESH_TOKEN}' \
    --ae 'CODEX_LOGIN_ID_TOKEN=${CODEX_LOGIN_ID_TOKEN}' \
    -o "$review" --job-name review > "$review/harbor.log" 2>&1 &
harbor_pid=$!
status=0
# A signal ends a wait early; Harbor is then asked to stop, and the waiting goes on until it has exited.
while kill -0 "$harbor_pid" 2> /dev/null; do
    if [ "$interrupted" = 1 ]; then
        kill -INT "$harbor_pid" 2> /dev/null || true
    fi
    wait "$harbor_pid" || status=$?
done
harbor_pid=""
if [ -f "$review/review/analysis.json" ]; then
    review/results.sh "$review" "$review/review" > "$review/results.txt" 2>&1 || status=1
else
    status=1
fi
review/codex/scan.sh "$private/auth.json" "$review" || stop "the review's output held the login or could not be read"
[ ! -f "$review/results.txt" ] || cat "$review/results.txt"
[ "$status" -eq 0 ] || echo "review: harbor analyze or the join failed; see $review/harbor.log" >&2
[ "$interrupted" = 0 ] || stop "interrupted; what the review wrote before it stopped has been scanned"
exit "$status"
