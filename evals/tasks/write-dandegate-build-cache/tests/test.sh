#!/usr/bin/env bash
set -euo pipefail

cd /app
verifier_dir="${VERIFIER_DIR:-${verifier_dir:-/logs/verifier}}"
mkdir -p "$verifier_dir"
export CARGO_NET_OFFLINE=true
export CARGO_BUILD_JOBS=4
export DANDEGATE_BIN=/app/target/debug/dandegate
work=$(mktemp -d /tmp/dandegate-cache-verifier.XXXXXX)
trap 'rm -rf "$work"' EXIT
cp -R /tests/harness "$work/harness"
export CARGO_TARGET_DIR="$work/target"

# Compile the candidate separately: the trusted scenarios never import its Rust source or tests.
build_ok=0
if timeout --kill-after=2s 90s env CARGO_TARGET_DIR=/app/target \
    cargo build --offline --locked -p dandegate --bin dandegate \
    > "$verifier_dir/build.log" 2>&1; then
    build_ok=1
fi

cd "$work/harness"
harness_ok=0
if timeout --kill-after=2s 60s cargo test --offline --locked \
    --manifest-path "$work/harness/Cargo.toml" --lib --no-run \
    > "$verifier_dir/harness-build.log" 2>&1; then
    harness_ok=1
fi

names=(
    caches::a_build_cache_is_kept_for_each_step_and_side_from_one_report_to_the_next
    caches::a_build_cache_another_report_holds_is_not_shared_and_the_page_says_so
    caches::the_caches_stay_locked_until_every_process_of_a_killed_report_has_ended
    caches::a_report_whose_running_steps_name_no_cache_leaves_the_caches_alone
    eviction::whole_caches_are_evicted_in_lru_order_to_the_byte_bound
)

if [[ "$harness_ok" = 1 ]]; then
    if ! timeout --kill-after=2s 10s cargo test --offline --locked \
        --manifest-path "$work/harness/Cargo.toml" --lib -- --list \
        > "$verifier_dir/scenario-list.log" 2>&1; then
        harness_ok=0
    fi
fi
: > "$verifier_dir/scenario-results.tsv"
for index in "${!names[@]}"; do
    name=${names[$index]}
    status=1
    if [[ "$build_ok" = 1 && "$harness_ok" = 1 ]]; then
        if timeout --kill-after=2s 20s cargo test --offline --locked \
            --manifest-path "$work/harness/Cargo.toml" --lib "$name" \
            -- --exact --test-threads=1 > "$verifier_dir/scenario-$index.log" 2>&1; then
            status=0
        fi
    fi
    printf '%s\t%s\n' "$name" "$status" >> "$verifier_dir/scenario-results.tsv"
done

score=$(python3 -I - "$verifier_dir" "$build_ok" "$harness_ok" <<'PY'
import json
import re
import sys
from pathlib import Path

logs = Path(sys.argv[1])
expected = {
    'caches::a_build_cache_is_kept_for_each_step_and_side_from_one_report_to_the_next',
    'caches::a_build_cache_another_report_holds_is_not_shared_and_the_page_says_so',
    'caches::the_caches_stay_locked_until_every_process_of_a_killed_report_has_ended',
    'caches::a_report_whose_running_steps_name_no_cache_leaves_the_caches_alone',
    'eviction::whole_caches_are_evicted_in_lru_order_to_the_byte_bound',
}
rows = [line.split('\t') for line in (logs / 'scenario-results.tsv').read_text().splitlines()]
listed = logs / 'scenario-list.log'
actual = set(re.findall(r'^(.+): test$', listed.read_text(), re.M)) if listed.exists() else set()
valid = (
    sys.argv[2:] == ['1', '1']
    and actual == expected
    and len(rows) == len(expected)
    and {row[0] for row in rows} == expected
)
results = {}
for index, (name, status) in enumerate(rows):
    output = logs / f'scenario-{index}.log'
    text = output.read_text() if output.exists() else ''
    passed = (
        valid
        and status == '0'
        and re.search(r'^test ' + re.escape(name) + r' \.\.\. ok$', text, re.M) is not None
        and re.search(r'^test result: ok\. 1 passed; 0 failed; 0 ignored;', text, re.M) is not None
    )
    results[name] = bool(passed)
(logs / 'functional.json').write_text(json.dumps({'valid_harness': valid, 'scenarios': results}, indent=2) + '\n')
frac = sum(results.values()) / len(expected)
print(frac if all(results.values()) else round(0.5 * frac, 4))
PY
)

base_rev=$(< /etc/eval-base-rev)
python3 -I /tests/grade_adherence.py write \
    --repo /app --base-rev "$base_rev" --manifest Cargo.toml \
    --functional-score "$score" --verifier-dir "$verifier_dir"
