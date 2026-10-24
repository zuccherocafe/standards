#!/usr/bin/env bash
set -euo pipefail

test_dir=${TEST_DIR:-/tests}
app_dir=${APP_DIR:-/app}
verifier_dir=${VERIFIER_DIR:-/logs/verifier}
base_rev=${EVAL_BASE_REV:-$(< /etc/eval-base-rev)}
export CARGO_NET_OFFLINE=true
mkdir -p "$verifier_dir" "$app_dir/tests"
contract="$app_dir/tests/harbor_image_contract.rs"
if [[ -e "$contract" ]]; then
    printf 'reserved verifier test path already exists: %s\n' "$contract" >&2
    exit 1
fi
cp "$test_dir/functional.rs" "$contract"
trap 'rm -f "$contract"' EXIT
cd "$app_dir"

# The contract checks dimensions, pixels, refusals and stale outputs through the serving API.
if cargo test --offline --locked --test harbor_image_contract -- --test-threads=1 \
    > "$verifier_dir/functional.log" 2>&1; then
    functional_status=0
else
    functional_status=$?
fi
if cargo clippy --offline --locked --all-targets -- -D warnings > "$verifier_dir/clippy.log" 2>&1; then
    clippy_pass=true
else
    clippy_pass=false
fi
functional_score=$(python3 - "$verifier_dir/functional.log" "$functional_status" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text()
results = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;", text)
if results:
    passed, failed = map(int, results[-1])
    frac = passed / (passed + failed) if passed + failed else 0.0
    print(frac if failed == 0 else round(0.5 * frac, 4))
else:
    print(0.0)
PY
)
printf '{"rust/documentation#clippy": {"applicable": true, "passed": %s, "detail": "clippy with -D warnings"}}\n' \
    "$clippy_pass" > "$verifier_dir/extra-rules.json"
python3 -I "$test_dir/grade_adherence.py" write \
    --repo "$app_dir" --base-rev "$base_rev" --manifest Cargo.toml \
    --functional-score "$functional_score" --extra-rules "$verifier_dir/extra-rules.json" \
    --verifier-dir "$verifier_dir"
printf 'functional score: %s\n' "$functional_score"
