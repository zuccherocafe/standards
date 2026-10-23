#!/bin/bash
# Copies the trials to review into review packets that do not say which agent, model or condition made each change,
# or whether it is the reference solution, for harbor analyze to read. Run it from evals/ with a new folder and the
# trial folders, in any order:
#
#   review/blind.sh <review folder> <trial folder>...
#
# <review folder>/blind/ is the job folder harbor analyze reads: one packet per trial, named by a random id, so the
# packets' order, which Harbor takes from their names, is shuffled too. <review folder>/key.json, outside it, maps
# each id to its trial, and <review folder>/publication.txt says what each packet's check found. A packet whose
# change names its trial's agent, model, condition or source paths is not published, and the script then exits 1
# once every trial is done. evals/README.md says what a packet holds, what it leaves out and what the check covers.
set -euo pipefail

stop() {
    echo "blind: $*" >&2
    exit 1
}

# grep, with a failure to read refused rather than taken for no match.
search() {
    local status=0
    grep "$@" || status=$?
    [ "$status" -le 1 ] || stop "grep $* failed"
}

[ "$#" -ge 2 ] || stop "usage: review/blind.sh <review folder> <trial folder>..."
review=$1
shift
[ -f configs/treatment.yaml ] || stop "run it from evals/"
[ ! -e "$review" ] || stop "$review already exists; give a new folder"

# The changes are reviewed against the rules and skills the treatment's bundle carries.
rev=$(sed -n 's|.*standards-\([0-9a-f]\{40\}\)$|\1|p' configs/treatment.yaml)
[ -n "$rev" ] || stop "configs/treatment.yaml names no standards revision"

mkdir -p "$review/blind"
: > "$review/blind/job.log"
echo '{}' > "$review/key.json"
: > "$review/publication.txt"
refused=0

for trial in "$@"; do
    trial=$(cd "$trial" && pwd)
    for file in trial.log result.json artifacts/var/tmp/model.patch verifier/reward.json verifier/test-stdout.txt; do
        [ -f "$trial/$file" ] || stop "$trial has no $file, which a review needs"
    done
    task=$(jq -er '.config.task.path' "$trial/result.json") || stop "$trial/result.json names no task path"
    [ -f "$task/task.toml" ] || stop "$trial's task $task is not a task folder under evals/"

    id=$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')
    packet="$review/blind/$id"
    [ ! -e "$packet" ] || stop "two trials drew the id $id; run it again"
    mkdir -p "$packet/task" "$packet/start" "$packet/verifier" "$packet/standards"

    # Harbor finds a trial by its trial.log; the packet's is empty.
    : > "$packet/trial.log"
    cp "$task/instruction.md" "$packet/task/"
    cp -r "$task/tests" "$packet/task/tests"
    cp "$trial/artifacts/var/tmp/model.patch" "$packet/change.patch"
    cp "$trial/verifier/reward.json" "$packet/verifier/"
    # Cargo's durations, such as 0.52s or 1m 02s, differ from run to run and say nothing about the change.
    sed -E 's/(finished|target\(s\)) in ([0-9]+h )?([0-9]+m )?[0-9]+(\.[0-9]+)?s/\1 in <time>/' \
        "$trial/verifier/test-stdout.txt" > "$packet/verifier/test-stdout.txt"
    git -C .. archive "$rev" rules skills | tar -x -C "$packet/standards"

    # The starting tree, rebuilt as the task's Dockerfile builds it and checked against the tree id task.toml records.
    base=$(sed -n 's/^source_base = "\([0-9a-f]*\)"$/\1/p' "$task/task.toml")
    tree=$(sed -n 's/^source_tree = "\([0-9a-f]*\)"$/\1/p' "$task/task.toml")
    index=$(mktemp)
    rm -f "$index"
    GIT_INDEX_FILE=$index git -C .. read-tree "$base"
    GIT_INDEX_FILE=$index git -C .. apply --cached --whitespace=nowarn "$PWD/$task/environment/start.patch"
    built=$(GIT_INDEX_FILE=$index git -C .. write-tree --prefix=tools/)
    rm -f "$index"
    [ "$built" = "$tree" ] || stop "$task's starting tree is $built, not the $tree task.toml records"
    git -C .. archive --prefix=tools/ "$tree" | tar -x -C "$packet/start"
    cp "$task/environment/AGENTS.md" "$packet/start/AGENTS.md"
    cp "$task/environment/gitignore" "$packet/start/.gitignore"

    # What the trial's record says identifies it: the model and each part of its name, the agent's environment
    # variable names, the condition's mounts as full paths, home-relative and without their ../ prefixes, and the
    # trial's own name, job and folder. Matched without regard to case; the agent's name only as a whole word, and
    # only from three letters, since a shorter one, such as pi, is an ordinary word in code.
    terms=$(jq -r --arg trial "$trial" '.config as $c
        | [($c.agent.model_name // empty | ., (split("/")[] | select(length >= 4))),
           ($c.agent.env // {} | keys[] | select(length >= 4)),
           ($c.environment.mounts // [] | .[] | (.target | ., sub("^/root/"; "~/"), (split("/")[0:3] | join("/"))),
             (.source | sub("^(\\.\\./)+"; ""))),
           $c.trial_name, ($c.trials_dir | split("/") | last), $trial]
        | map(select(length >= 4)) | unique | sort_by(-length)[]' "$trial/result.json")
    words=$(jq -r '.config.agent.name // empty | select(length >= 3)' "$trial/result.json")
    # The paths the change leaves, as git apply decodes them from Git's quoting, with each binary change marked:
    # an empty or binary file has a name and no added line. git apply runs from / because inside a repository it
    # skips the paths outside the folder it runs in. Only what the change adds can name the trial; the lines it
    # removes and its context come from the start.
    stats=$(git -C / apply --numstat -z "$(cd "$packet" && pwd)/change.patch" | tr '\0' '\n') \
        || stop "$trial's change cannot be read as a patch"
    tab=$(printf '\t')
    paths=$(printf '%s\n' "$stats" | sed -E "s/^([0-9]+|-)$tab([0-9]+|-)$tab//" | sed '/^$/d')
    added=$(search -E '^(\+|rename to |copy to )' "$packet/change.patch")
    found=$( { printf '%s\n' "$paths" "$added" | search -inF -f <(printf '%s\n' "$terms")
               [ -z "$words" ] || printf '%s\n' "$paths" "$added" | search -inwF -f <(printf '%s\n' "$words"); } )
    binary=$(printf '%s\n' "$stats" | search -E "^-$tab-$tab")
    if [ -n "$found" ] || [ -n "$binary" ]; then
        rm -rf "$packet"
        refused=$((refused + 1))
        { [ -z "$found" ] || { echo "$trial: not published; a path or added line names the trial:"
                                 printf '%s\n' "$found"; }
          [ -z "$binary" ] || { echo "$trial: not published; a binary change cannot be checked:"
                                  printf '%s\n' "$binary"; }; } >> "$review/publication.txt"
        continue
    fi
    # The verifier's output repeats the change and the tests, so a name there is withheld rather than refused,
    # by the same two matches.
    output="$packet/verifier/test-stdout.txt"
    withheld=$( { search -inF -f <(printf '%s\n' "$terms") "$output"
                  [ -z "$words" ] || search -inwF -f <(printf '%s\n' "$words") "$output"; } \
        | cut -d: -f1 | sort -u | wc -l)
    if [ "$withheld" != 0 ]; then
        while IFS= read -r term; do
            [ -n "$term" ] || continue
            # shellcheck disable=SC2001 # the term is a literal, escaped for sed first
            pattern=$(printf '%s' "$term" | sed 's/[][\/.^$*]/\\&/g')
            sed -i "s/$pattern/[withheld]/gI" "$output"
        done <<< "$terms"
        while IFS= read -r word; do
            [ -n "$word" ] || continue
            # shellcheck disable=SC2001 # the word is a literal, escaped for sed first
            pattern=$(printf '%s' "$word" | sed 's/[][\/.^$*]/\\&/g')
            sed -i "s/\\b$pattern\\b/[withheld]/gI" "$output"
        done <<< "$words"
    fi
    echo "$trial: published as $id; $withheld verifier output lines had a name withheld" >> "$review/publication.txt"

    # A trial runs the treatment when its environment mounts the standards bundle at ~/.agents, as
    # configs/treatment.yaml does; Harbor's oracle agent applies the task's reference solution.
    jq --arg id "$id" --arg trial "$trial" --slurpfile result "$trial/result.json" \
        '.[$id] = ($result[0].config | {trial: $trial, job: ($trial | split("/") | .[-2]), task: .task.path,
          agent: .agent.name, model: .agent.model_name,
          condition: (if any(.environment.mounts[]?; .target == "/root/.agents") then "treatment" else "control" end),
          reference: (.agent.name == "oracle")})' \
        "$review/key.json" > "$review/key.json.part"
    mv "$review/key.json.part" "$review/key.json"
done

echo "blind: $(( $# - refused )) packets in $review/blind, key in $review/key.json, report in $review/publication.txt"
[ "$refused" -eq 0 ] || stop "$refused of $# trials not published; $review/publication.txt says why"
