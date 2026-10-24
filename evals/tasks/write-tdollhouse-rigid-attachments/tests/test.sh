#!/usr/bin/env bash
set -euo pipefail

tests_dir="${TESTS_DIR:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)}"
app_dir="${APP_DIR:-/app}"
verifier_dir="${VERIFIER_DIR:-/logs/verifier}"
grade_script="${GRADE_SCRIPT:-$tests_dir/grade_adherence.py}"
if [[ ! -f "$grade_script" ]]; then
    grade_script="$tests_dir/../../../runner/grade_adherence.py"
fi
base_rev="${BASE_REV:-}"
if [[ -z "$base_rev" ]]; then
    base_rev="$(< "${BASE_REV_FILE:-/etc/eval-base-rev}")"
fi
mkdir -p "$verifier_dir" "$app_dir/tests"
contract="$app_dir/tests/harbor_contract.rs"
cp "$tests_dir/contract.rs" "$contract"
trap 'rm -f -- "$contract"' EXIT

if (cd "$app_dir" && cargo test --locked --offline -p tdoll-attachments --test harbor_contract) \
    > "$verifier_dir/functional.log" 2>&1; then
    true
fi
functional=$(python3 -I - "$verifier_dir/functional.log" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
results = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;", text)
if results:
    passed, failed = map(int, results[-1])
    frac = passed / (passed + failed) if passed + failed else 0.0
    print(frac if failed == 0 else round(0.5 * frac, 4))
else:
    print(0.0)
PY
)
printf 'Functional score: %s\n' "$functional"
cat "$verifier_dir/functional.log"
python3 -I "$grade_script" write \
    --repo "$app_dir" \
    --base-rev "$base_rev" \
    --manifest Cargo.toml \
    --core tdoll-attachments \
    --core tdoll-springs \
    --functional-score "$functional" \
    --verifier-dir "$verifier_dir"
