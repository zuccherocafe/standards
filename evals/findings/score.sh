#!/bin/bash
# Scores a review task: the findings a review left, one JSON object per line, against the task's known defects. It uses
# no model, so a file scores the same every time. Usage: score.sh <findings.jsonl> <expected.json> <reward.json>.
#
# A finding is {"file", "lines": [first, last], "kind", "severity", "finding"}. It is on a defect when its file, less
# any leading ./, is the file of one of the defect's locations, it spans at most $span lines, and its lines meet that
# location widened by $slack lines on each side; it finds the defect when it is also must-fix and of the defect's kind.
# The file is read as one finding per line: an empty file holds none, and every other line, a blank one included, must
# be a finding, after an optional newline at the end of the last.
#
# `reward`, the known-defect coverage, is 1 when every known defect is found, or, for a task with none, when there is
# no must-fix finding. `precision` is the defects found over the defects and the must-fix findings that find none, so
# a review that accuses everything scores near 0 and repeating a finding on a found defect changes nothing; for a task
# with none, it is `reward`. A missing file, or one with any line that is not a finding, scores 0 on both. Each count
# beside them goes into reward.json, so Harbor averages it over a job's trials.
set -uo pipefail

findings=$1 expected=$2 reward=$3
slack=3
span=30
kinds='["correctness", "concurrency", "security", "performance", "tests", "documentation", "design", "idiom"]'

mkdir -p "$(dirname "$reward")"
if [ ! -f "$findings" ]; then
    echo "score: the review left no findings file"
    echo '{"reward": 0, "precision": 0, "findings_captured": 0, "findings_valid": 0}' > "$reward"
    exit 0
fi
jq -n -c --rawfile text "$findings" --slurpfile expected "$expected" --argjson kinds "$kinds" \
    --argjson slack "$slack" --argjson span "$span" '
    def whole: type == "number" and . == floor and . >= 1;
    def valid:
        type == "object" and (keys == ["file", "finding", "kind", "lines", "severity"])
        and (.file | type == "string" and length > 0 and (startswith("/") | not))
        and (.lines | type == "array" and length == 2 and all(.[]; whole) and .[0] <= .[1])
        and (.kind as $kind | $kinds | index([$kind]) != null)
        and (.severity == "must-fix" or .severity == "should-fix")
        and (.finding | type == "string" and length > 0);
    def on($location):
        (.file | ltrimstr("./")) == $location.file and (.lines[1] - .lines[0] + 1) <= $span
        and .lines[0] <= $location.end + $slack and .lines[1] >= $location.start - $slack;
    def finds($defect):
        .severity == "must-fix" and .kind == $defect.kind
        and (. as $finding | any($defect.locations[]; . as $at | $finding | on($at)));
    (if $text == "" then [] else $text | split("\n") | if last == "" then .[:-1] else . end end) as $lines
    | ($lines | map(try fromjson catch null)) as $parsed
    | $expected[0].defects as $defects
    | if all($parsed[]; valid) | not then
        {reward: 0, precision: 0, findings_captured: 1, findings_valid: 0, findings: ($lines | length)}
      else
        ($parsed | map(select(.severity == "must-fix"))) as $must
        | ($defects | map(. as $defect | select(any($must[]; finds($defect)))) | length) as $matched
        | ($must | map(. as $finding | select(all($defects[]; . as $defect | $finding | finds($defect) | not)))
          | length) as $unmatched
        | {reward: (if ($defects | length) > 0 then (if $matched == ($defects | length) then 1 else 0 end)
                    else (if ($must | length) == 0 then 1 else 0 end) end),
           precision: (if ($defects | length) > 0 then $matched / (($defects | length) + $unmatched)
                       else (if ($must | length) == 0 then 1 else 0 end) end),
           findings_captured: 1, findings_valid: 1, findings: ($parsed | length), must_fix: ($must | length),
           should_fix: ($parsed | map(select(.severity == "should-fix")) | length),
           defects: ($defects | length), defects_matched: $matched, unmatched_must_fix: $unmatched}
      end' > "$reward.part" || { echo "score: cannot read the findings or the expected defects"; exit 1; }
mv "$reward.part" "$reward"
echo "score: $(cat "$reward")"
