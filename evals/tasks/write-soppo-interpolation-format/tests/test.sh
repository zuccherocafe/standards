#!/usr/bin/env bash
set -euo pipefail
repo=${SOPPO_REPO:-/app}
tests_dir=${TESTS_DIR:-/tests}
base_rev_file=${BASE_REV_FILE:-/etc/eval-base-rev}
verifier_dir=${VERIFIER_DIR:-${verifier_dir:-/logs/verifier}}
mkdir -p "$verifier_dir"
SCORE=0.0
suite=$(mktemp -d)
trap 'rm -rf -- "$suite"' EXIT
mkdir -p "$suite/tests"
cp "$tests_dir/acceptance.rs" "$suite/tests/acceptance.rs"
python3 -I - "$repo" "$suite" <<'PY'
import json
import sys
from pathlib import Path

repo = Path(sys.argv[1]).resolve()
suite = Path(sys.argv[2])
suite.joinpath("Cargo.toml").write_text(
    '[package]\nname = "interpolation-acceptance"\nversion = "0.0.0"\nedition = "2024"\n'
    '[workspace]\nresolver = "3"\n[dependencies]\nsoppo = { path = '
    + json.dumps(str(repo))
    + ' }\n[profile.dev]\ndebug = 0\n[profile.test]\ndebug = 0\n',
    encoding="utf-8",
)
PY
if CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}" \
    cargo test --offline --manifest-path "$suite/Cargo.toml" --test acceptance > "$verifier_dir/functional.log" 2>&1; then
    true
fi
SCORE=$(python3 -I - "$verifier_dir/functional.log" <<'PY'
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
cat "$verifier_dir/functional.log"
python3 -I - "$repo" /tmp/extra_rules.json <<'PY'
import json
import re
import sys
from pathlib import Path

repo = Path(sys.argv[1])
source = "\n".join(path.read_text(encoding="utf-8") for path in sorted((repo / "src").rglob("*.rs")))
source = re.sub(r"//[^\n]*", "", source)
models = re.findall(r"\b(?:struct|enum)\s+(\w*(?:Format|Directive)\w*)\b", source)
structured = False
for model in models:
    declaration = re.search(rf"\b(?:struct|enum)\s+{model}\s*\{{([^}}]*)\}}", source, re.DOTALL)
    ast_storage = re.search(rf"\b(?:format|spec|directive)\s*:\s*Option\s*<[^>]*\b{model}\b", source)
    display = re.search(rf"\bimpl\s+(?:\w+::)*Display\s+for\s+{model}\b", source)
    if declaration and ast_storage and display:
        fields = declaration.group(1)
        structured = not re.search(r"\b(?:String|str)\b|\bpub\b", fields)
        if structured:
            break
string_error = bool(re.search(r"Result\s*<\s*\(\s*\)\s*,\s*String\s*>", source))
scenarios = repo / "tests/scenarios.rs"
scenario_text = scenarios.read_text(encoding="utf-8") if scenarios.is_file() else ""
read = repo / "docs/concord/interpolation.md"
read_text = read.read_text(encoding="utf-8") if read.is_file() else ""
section = read_text.partition("## Disagreements")[2].split("\n## ", 1)[0].strip()
rules = {
    "rust/design#invariants": {
        "passed": structured and not string_error,
        "detail": "AST stores a structured directive with private components and Display, not a raw string validator",
    },
    "rust/tests#scenarios-first": {
        "passed": scenario_text.lstrip().startswith("//!") and "#[test]" in scenario_text,
        "detail": "tests/scenarios.rs documents the public uses and contains scenario tests",
    },
    "skills/concord#settled-interpolation": {
        "passed": section == "None found.",
        "detail": "interpolation disagreement is settled without leaving a table beside None found",
    },
}
Path(sys.argv[2]).write_text(json.dumps(rules, indent=2) + "\n", encoding="utf-8")
PY
python3 -I "$tests_dir/grade_adherence.py" write --repo "$repo" \
    --base-rev "$(cat "$base_rev_file")" --functional-score "$SCORE" --core soppo \
    --extra-rules /tmp/extra_rules.json --verifier-dir "$verifier_dir"
