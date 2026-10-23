#!/bin/bash
# Joins a harbor analyze job's verdicts with the key review/blind.sh kept, writes them with the reviewer that gave
# them to <review folder>/results.json, and prints each trial's verdicts, one column per criterion, with each
# attempt's outcome in order. A criterion agrees for a trial when every attempt that finished gave it one outcome.
# It refuses a key and an analysis that do not describe the same packets the same number of times.
#
#   review/results.sh <review folder> <analysis job folder>
set -euo pipefail

stop() {
    echo "results: $*" >&2
    exit 1
}

[ "$#" -eq 2 ] || stop "usage: review/results.sh <review folder> <analysis job folder>"
review=$1 job=$2
for file in "$review/key.json" "$job/analysis.json" "$job/config.json"; do
    [ -f "$file" ] || stop "no $file"
done

# The key and the analysis must describe the same packets, each analysed as often as the job asked.
# Harbor leaves n_attempts out of config.json at its default, 1.
attempts=$(jq -er '.n_attempts // 1' "$job/config.json") || stop "cannot read $job/config.json"
mismatch=$(jq -nr --slurpfile key "$review/key.json" --slurpfile analysis "$job/analysis.json" --argjson n "$attempts" '
    ($key[0] | keys) as $ids
    | ($analysis[0].results | group_by(.trial_name) | map({key: .[0].trial_name, value: length})
       | from_entries) as $count
    | ([$ids[] | select($count[.] == null) | "\(.): in the key, not analysed"]
       + [$count | keys[] | select(. as $id | $ids | index($id) | not) | "\(.): analysed, not in the key"]
       + [$count | to_entries[] | select(.value != $n) | "\(.key): \(.value) results, not \($n)"])[]')
[ -z "$mismatch" ] || stop "$review/key.json and $job/analysis.json do not match:
$mismatch"

versions=$(cat "$job"/*/result.json | jq -s '[.[].agent_info.version] | unique')
# A bug-fix task's reference is its correction's production part alone, so its tests are no baseline; a feature
# task's reference carries the landed change's tests.
bug_fixes=$(grep -l '^keywords = .*"review-finding"' tasks/*/task.toml | xargs -r -n 1 dirname | xargs -r -n 1 basename \
    | jq -R . | jq -s .) || stop "cannot read the tasks' keywords"

jq -n --slurpfile key "$review/key.json" --slurpfile analysis "$job/analysis.json" \
    --slurpfile config "$job/config.json" --argjson versions "$versions" --argjson bug_fixes "$bug_fixes" \
    --arg job "$job" '
    def agreement:
        [.[] | select(.error == null) | .checks] as $done
        | if ($done | length) < 2 then null
          else $done[0] | keys | map({key: ., value: (. as $c | [$done[][$c].outcome] | unique | length == 1)})
            | from_entries
          end;
    ($analysis[0].results | group_by(.trial_name) | map({key: .[0].trial_name, value: .}) | from_entries) as $by_id
    | {reviewer: ($config[0].agents[0] | {agent: .name, model: .model_name, kwargs, versions: $versions}),
       analysis_job: $job,
       trials: [$key[0] | to_entries[] | .key as $id
         | .value + {id: $id, attempts: [$by_id[$id][] | {summary, checks, error}]}
         | .agreement = (.attempts | agreement)
         | (.task | split("/") | last) as $task
         | if .reference and ($bug_fixes | index([$task]) != null)
           then .baseline = {regression_test: "unavailable: the reference of a bug-fix task omits its tests by design"}
           else . end]}
    | .agreement = ([.trials[].agreement | select(. != null) | to_entries[]] | group_by(.key)
        | map({key: .[0].key, value: {agreed: map(select(.value)) | length, compared: length}}) | from_entries)
    ' > "$review/results.json.part"
mv "$review/results.json.part" "$review/results.json"

jq -r '
    (.trials | map(.attempts[] | select(.error == null) | .checks | keys) | add // [] | unique) as $criteria
    | (["task", "agent", "condition"] + $criteria | @tsv),
      (.trials | sort_by(.task, .agent)[]
        | . as $t
        | [.task, .agent, .condition]
          + [$criteria[] as $c | [$t.attempts[] | if .error then "error" else .checks[$c].outcome end] | join("/")]
        | @tsv),
      "",
      "agreed of compared, by criterion",
      (.agreement | to_entries[] | "\(.key)\t\(.value.agreed) of \(.value.compared)")
    ' "$review/results.json"
