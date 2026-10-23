#!/bin/bash
# Looks for a Codex login's access, refresh and ID tokens under the folders given, in every path name and in every
# file's contents: each token whole, every 24-character run of it, its base64 and base64url at each of the three
# alignments a token can take inside a longer encoded text, and its hex. It deletes each path that holds one, and each
# file it cannot read or that is not a regular file, a folder or a link, and then exits 1. It names a path only once
# its name is known to be clean, and keeps the diagnostics of find, grep and rm, which name paths, to itself. It refuses
# a folder given as a link, which find would not enter. It exits 0 only when every path was inspected and no
# configured pattern was found. This is best-effort detection, not a boundary: evals/README.md says what it cannot
# catch. Run it from evals/:
#
#   review/codex/scan.sh <auth.json> <folder>...
set -euo pipefail

stop() {
    echo "scan: $*" >&2
    exit 1
}

[ "$#" -ge 2 ] || stop "usage: review/codex/scan.sh <auth.json> <folder>..."
auth=$1
shift
[ -r "$auth" ] || stop "cannot read $auth"
for folder in "$@"; do
    [ ! -L "$folder" ] || stop "$folder is a link; give the folder it points to"
    [ -d "$folder" ] || stop "$folder is not a folder"
done

# The patterns live in a private folder for the length of the scan, never in an argument or the output.
private=$(mktemp -d)
chmod 700 "$private"
trap 'rm -rf "$private"' EXIT
patterns=$private/patterns
: > "$patterns"
for field in access_token refresh_token id_token; do
    token=$(jq -er ".tokens.$field" "$auth") || stop "$auth has no tokens.$field"
    [ "${#token}" -ge 24 ] || stop "$auth's tokens.$field is too short to be a token"
    {
        printf '%s\n' "$token"
        for ((start = 0; start + 24 <= ${#token}; start++)); do
            printf '%s\n' "${token:start:24}"
        done
        for pad in "" "x" "xx"; do
            encoded=$(printf '%s%s' "$pad" "$token" | base64 -w 0)
            # Drop the characters the padding or the end shares with what surrounds the token.
            encoded=${encoded:4:${#encoded}-8}
            printf '%s\n%s\n' "$encoded" "$(printf '%s' "$encoded" | tr '+/' '-_')"
        done
        hex=$(printf '%s' "$token" | od -An -tx1 | tr -d ' \n')
        printf '%s\n%s\n' "$hex" "$(printf '%s' "$hex" | tr 'a-f' 'A-F')"
    } >> "$patterns"
done

# grep -F: 0 when a pattern is found, 1 when none is, and anything else when it could not read.
holds() {
    local status=0
    grep -qF -f "$patterns" "$@" 2> /dev/null || status=$?
    [ "$status" -le 1 ] || return 2
    return "$status"
}

# Deletes a path, a folder its owner cannot read included; returns non-zero, silently, when it cannot. A link is
# removed without touching what it points to; in a folder, only the folders find reaches without following a link
# are opened up for removal, and no file's mode changes.
delete() {
    if [ -L "$1" ]; then
        rm -f -- "$1" 2> /dev/null
    else
        if [ -d "$1" ]; then
            find "$1" -type d -exec chmod u+rwx {} ';' 2> /dev/null || true
        fi
        rm -rf -- "$1" 2> /dev/null
    fi
    [ ! -e "$1" ] && [ ! -L "$1" ]
}

failures=0
inspected=0
# The listing ends with find's own exit status, so a folder it could not enter fails the scan; find's messages, which
# name the paths it could not read, go nowhere.
mapfile -d '' -t entries < <(
    listed=0
    find "$@" -mindepth 1 -print0 2> /dev/null || listed=$?
    printf 'find-status:%s' "$listed"
)
[ "${entries[-1]}" = "find-status:0" ] || { failures=$((failures + 1)); echo "scan: could not list every path" >&2; }
unset 'entries[-1]'
for path in "${entries[@]}"; do
    [ -e "$path" ] || [ -L "$path" ] || continue
    name_status=0
    printf '%s' "$path" | holds || name_status=$?
    if [ "$name_status" -ne 1 ]; then
        failures=$((failures + 1))
        if delete "$path"; then
            echo "scan: deleted a path whose name holds the login or could not be checked" >&2
        else
            echo "scan: could not delete a path whose name holds the login or could not be checked" >&2
        fi
        continue
    fi
    if [ -L "$path" ]; then
        target_status=0
        readlink -- "$path" 2> /dev/null | holds || target_status=$?
        [ "$target_status" -eq 1 ] && continue
        why="its link target holds the login"
    elif [ -d "$path" ]; then
        continue
    elif [ -f "$path" ] && [ -r "$path" ]; then
        inspected=$((inspected + 1))
        content_status=0
        holds -- "$path" || content_status=$?
        [ "$content_status" -eq 1 ] && continue
        why=$([ "$content_status" -eq 0 ] && echo "it holds the login" || echo "it could not be read")
    else
        why="it is not a readable regular file, folder or link"
    fi
    failures=$((failures + 1))
    if delete "$path"; then
        echo "scan: deleted $path; $why" >&2
    else
        echo "scan: could not delete $path; $why" >&2
    fi
done

[ "$failures" -eq 0 ] \
    || stop "$failures paths held the login or could not be listed, read or deleted; treat the login as exposed"
echo "scan: no configured pattern found in $inspected files under $*"
