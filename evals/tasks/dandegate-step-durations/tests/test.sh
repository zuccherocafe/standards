#!/bin/bash
# Scores the agent's change in a fresh copy of the task's image, without network. It applies the change the agent
# phase captured as /var/tmp/model.patch to the clean starting tree, then the hidden tests, and scores 1 only
# when the existing tests and the hidden tests both pass. Each stage's result goes into reward.json beside the score.
set -uo pipefail

manifest=tools/Cargo.toml
crate=dandegate
# The hidden tests: the integration-test target that holds them, and the module test.patch adds to it.
target=evaluation
target_path=/app/tools/crates/dandegate/tests/evaluation.rs
hidden=evaluation::
# How many tests test.patch adds there, every one of which must run and pass.
hidden_count=5
# This existing test mounts /proc in a new PID namespace, which the rootless container forbids, so it fails here
# whatever the change.
skips=(--skip scheduling::two_reports_on_one_repository_each_in_its_own_pid_namespace_keep_their_own_checkouts)
# Each Cargo run's own deadline in seconds, under the verifier's: building, the existing tests, the hidden tests,
# which take seconds when they pass. timeout stops the run's whole process group.
declare -A deadlines=([build]=900 [existing]=600 [hidden]=180)
declare -A stage=([patch_captured]=0 [patch_applied]=0 [tests_applied]=0 [hidden_target]=0 [built]=0
    [existing_tests]=0 [hidden_tests]=0 [timed_out]=0)
finish() {
    local reward=$((stage[existing_tests] * stage[hidden_tests])) fields="" name
    for name in "${!stage[@]}"; do fields+=", \"$name\": ${stage[$name]}"; done
    mkdir -p /logs/verifier
    echo "{\"reward\": $reward$fields}" > /logs/verifier/reward.json
    echo "verifier: reward $reward$fields"
}
trap finish EXIT

cargo_run() {
    local deadline=${deadlines[$1]}
    shift
    timeout --kill-after=10 "$deadline" cargo test --offline --manifest-path "$manifest" -p "$crate" "$@"
    local status=$?
    if [ "$status" -eq 124 ] || [ "$status" -eq 137 ]; then stage[timed_out]=1; fi
    return "$status"
}

cd /app || exit 1
patch=/var/tmp/model.patch
if [ ! -f "$patch" ]; then
    echo "verifier: the agent phase left no $patch"
    exit 0
fi
stage[patch_captured]=1
if [ -s "$patch" ] && ! git apply --whitespace=nowarn "$patch"; then
    echo "verifier: the agent's change does not apply to the starting tree"
    exit 0
fi
stage[patch_applied]=1

# The hidden tests replace whatever the agent left at their paths.
for file in $(sed -n 's|^diff --git a/\(.*\) b/.*$|\1|p' /tests/test.patch); do
    git checkout -q HEAD -- "$file" 2>/dev/null || rm -rf -- "$file"
done
if ! git apply --whitespace=nowarn /tests/test.patch; then
    echo "verifier: the hidden tests do not apply"
    exit 0
fi
stage[tests_applied]=1

# The hidden target must be the crate's own integration test at its own path, which the reset above restored, so a
# manifest change cannot point its name at other tests. That file has no main function, so it builds only under
# Cargo's standard test harness.
registered=$(cargo metadata --offline --no-deps --format-version 1 --manifest-path "$manifest" \
    | jq -c --arg crate "$crate" --arg target "$target" \
        '[.packages[] | select(.name == $crate) | .targets[] | select(.name == $target)
          | {kind, src_path, features: (."required-features" // [])}]')
if [ "$registered" != "[{\"kind\":[\"test\"],\"src_path\":\"$target_path\",\"features\":[]}]" ]; then
    echo "verifier: the hidden target $target is registered as ${registered:-nothing readable}, not as $target_path"
    exit 0
fi
stage[hidden_target]=1

cargo_run build --no-run || exit 0
stage[built]=1
cargo_run existing -- --skip "$hidden" "${skips[@]}" && stage[existing_tests]=1
# Named by its target, so a manifest that stops discovering it fails rather than running nothing.
cargo_run hidden --test "$target" -- "$hidden" > /tmp/hidden-tests.log 2>&1
status=$?
cat /tmp/hidden-tests.log
passed=$(sed -n 's/^test result: ok\. \([0-9]*\) passed.*/\1/p' /tmp/hidden-tests.log)
echo "verifier: ${passed:-0} of $hidden_count hidden tests passed"
if [ "$status" -eq 0 ] && [ "${passed:-0}" -eq "$hidden_count" ]; then stage[hidden_tests]=1; fi
exit 0
