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

# Removes, by ID, every container and network left from an analysis, each of which can hold the login. An interrupted
# Harbor can exit with an analysis's containers still running, so this runs whenever Harbor has exited. Harbor names
# each analysis's Compose project after its trial's folder with "__env" added, in lower case, with a 0 before a name
# that does not start with a letter or a digit, and with every character but letters, digits, - and _ made a -. Only
# objects with that exact project label are removed: the main container first, since it shares the egress sidecar's
# network, then the rest. A container of another owner that depends on one of them makes the removal fail rather than
# go with it.
remove_leftovers() {
    local trial project label service ids failed=0
    for trial in "$review/review"/*/; do
        [ -d "$trial" ] || continue
        project=$(basename "$trial")__env
        project=${project,,}
        [[ $project =~ ^[a-z0-9] ]] || project=0$project
        project=${project//[^a-z0-9_-]/-}
        label=label=com.docker.compose.project=$project
        for service in "--filter=label=com.docker.compose.service=main" ""; do
            # shellcheck disable=SC2086 # the service filter is one argument or none
            if ids=$(podman ps -aq --no-trunc --filter "$label" $service); then
                # shellcheck disable=SC2086 # one argument per ID
                [ -z "$ids" ] || podman rm -f -t 0 $ids > /dev/null || failed=1
            else
                failed=1
            fi
        done
        if ids=$(podman network ls --no-trunc --format '{{.ID}}' --filter "$label"); then
            # shellcheck disable=SC2086 # one argument per ID
            [ -z "$ids" ] || podman network rm $ids > /dev/null || failed=1
        else
            failed=1
        fi
    done
    return "$failed"
}

# Harbor runs in a transient systemd user scope of its own, named for this review, in the slice this script runs in,
# so every process it starts, Compose's among them, is in that scope's cgroup. The scope is signalled and stopped
# through systemd, which acts on the cgroup and so never on a process outside it, whatever becomes of a process ID.
unit=evals-review-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n').scope
harbor_started=0
harbor_stopped=0
kept=0
# Stops Harbor's scope: SIGTERM to every process in it, SIGKILL to what remains after ten seconds, and success only
# once systemd reports the scope inactive, which it does when its cgroup is empty; a scope still there ten seconds
# after SIGKILL is a failure.
stop_harbor() {
    systemctl --user stop "$unit" 2> /dev/null || true
    [ "$(systemctl --user show -p ActiveState --value "$unit" 2> /dev/null)" = inactive ] || return 1
    harbor_stopped=1
}
# What the review can still be recovered from when Harbor's scope has not stopped: its name and the login copy the
# scan needs, both kept. evals/README.md says how to recover.
incomplete() {
    kept=1
    echo "review: Harbor's scope $unit has not stopped, so the analyses' containers and output may still change" \
        "and the scan is incomplete; nothing is published. The login copy stays at $private/auth.json until" \
        "you recover as evals/README.md says." >&2
}

private=$(mktemp -d)
chmod 700 "$private"
# shellcheck disable=SC2329 # the EXIT trap below runs it
finish() {
    if [ "$kept" = 0 ] && [ "$harbor_started" = 1 ] && [ "$harbor_stopped" = 0 ]; then
        if stop_harbor; then
            remove_leftovers || echo "review: an analysis's container or network may remain; remove it by ID" >&2
        else
            incomplete
        fi
    fi
    [ "$kept" = 1 ] || rm -rf "$private"
}
trap finish EXIT
interrupted=0
trap 'interrupted=$((interrupted + 1))' INT TERM

cp "$login" "$private/auth.json"
export CODEX_AUTH_JSON_PATH=$private/auth.json
CODEX_LOGIN_ACCESS_TOKEN=$(jq -er '.tokens.access_token' "$private/auth.json") || stop "the login has no access token"
CODEX_LOGIN_REFRESH_TOKEN=$(jq -er '.tokens.refresh_token' "$private/auth.json") \
    || stop "the login has no refresh token"
CODEX_LOGIN_ID_TOKEN=$(jq -er '.tokens.id_token' "$private/auth.json") || stop "the login has no ID token"
export CODEX_LOGIN_ACCESS_TOKEN CODEX_LOGIN_REFRESH_TOKEN CODEX_LOGIN_ID_TOKEN

# An interrupt that came while the login was being prepared stops the review before Harbor starts.
[ "$interrupted" = 0 ] || stop "interrupted before Harbor started; nothing ran"
set -m
# A background job of a shell without job control ignores SIGINT, so Harbor would never act on the interrupt passed on
# below; with job control on, Harbor starts with SIGINT's default action in a process group of its own, which a
# terminal's interrupt does not reach, so Harbor gets only the interrupts this script passes on; it reads nothing from
# the terminal. The single quotes keep each value a template, which Harbor fills from the environment and records
# unfilled; Harbor treats the names as secrets and replaces their values with [REDACTED] in each trial's files.
harbor_started=1
# shellcheck disable=SC2016
systemd-run --user --scope --quiet --slice-inherit --expand-environment=no --unit="$unit" -p TimeoutStopSec=10s -- \
    harbor analyze "$review/blind" -r review/rubric.toml -p review/prompt.txt --task-config review/codex/task.toml \
    -e podman -n "$at_once" -k 2 -a codex -m gpt-6-astra --ak version=0.163.0 --ak reasoning_effort=high \
    --ae 'CODEX_LOGIN_ACCESS_TOKEN=${CODEX_LOGIN_ACCESS_TOKEN}' \
    --ae 'CODEX_LOGIN_REFRESH_TOKEN=${CODEX_LOGIN_REFRESH_TOKEN}' \
    --ae 'CODEX_LOGIN_ID_TOKEN=${CODEX_LOGIN_ID_TOKEN}' \
    -o "$review" --job-name review < /dev/null > "$review/harbor.log" 2>&1 &
harbor_job=$!
set +m
forwarded=0
# Each interrupt is passed on to every process in Harbor's scope, as a terminal's interrupt would reach a foreground
# job, and counts as passed on only once systemd has taken it, so one that comes before the scope exists stays pending
# until it does. The script never blocks for more than half a second at a time, so no pending interrupt waits behind a
# wait that began just after it came; it goes on until Harbor has exited.
while jobs -pr | grep -qx "$harbor_job"; do
    if [ "$forwarded" -lt "$interrupted" ] && systemctl --user kill -s INT "$unit" 2> /dev/null; then
        forwarded=$((forwarded + 1))
        continue
    fi
    sleep 0.5 &
    wait "$!" || true
done
status=0
wait "$harbor_job" || status=$?
# Nothing is removed, scanned as complete or published until every process of Harbor's has gone.
if ! stop_harbor; then
    review/codex/scan.sh "$private/auth.json" "$review" || true
    incomplete
    exit 1
fi
if ! remove_leftovers; then
    status=1
    echo "review: an analysis's container or network may remain; remove it by ID" >&2
fi
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
