#!/usr/bin/env bash
set -euo pipefail
solution_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo=${1:-/app}
cd -- "$repo"
git apply --check "$solution_dir/implementation.patch"
git apply "$solution_dir/implementation.patch"
python3 - <<'PY'
from pathlib import Path
import subprocess

revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
read = Path("docs/concord/interpolation.md")
read.parent.mkdir(parents=True, exist_ok=True)
read.write_text(f"""# Interpolation design read

- Area: interpolated strings from Soppo source through Go generation and source formatting
- Last full read: {revision}
- Inspected: src/syntax/format_spec.rs, src/syntax/ast.rs, src/syntax/parser/expr.rs, src/types/infer/expr.rs, src/codegen/expr.rs, src/fmt.rs and tests/scenarios.rs

## Models in play

`StringPart` owns literal text or an expression with an optional checked `FormatSpec`. The directive's private components represent its verb, sign, alternate form, padding, width and precision. Its constructor rejects malformed directives at the syntax boundary.

## Overlaps

The syntax tree carries one directive representation through inference, code generation and source formatting. Type inference asks the directive whether it accepts the inferred expression type. Both renderers use its `Display` implementation without parsing directive text again.

## Half-done transitions

None found. Every interpolation consumer handles the same optional checked directive, and expressions without a directive retain the general `%v` rendering.

## What later work needs

Additional verbs need grammar, type compatibility, rendering and public-boundary scenarios in the same change. Import catalogue expansion belongs to the single-source compiler boundary rather than interpolation syntax.

## Disagreements

None found.
""", encoding="utf-8")
PY
cargo fmt --check
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
if command -v concord >/dev/null 2>&1; then
    concord check
fi
if command -v rust-scans >/dev/null 2>&1; then
    rust-scans --workspace . --core soppo
fi
git add src tests/scenarios.rs docs/concord/interpolation.md
git -c user.name='Reference solution' -c user.email='reference@example.invalid' commit \
    -m 'syntax: add typed interpolation format specifiers' \
    -m 'Parse interpolation directives into checked syntax shared by inference, Go generation and source formatting. Refuse malformed directives at parsing and incompatible expression types during inference, with typed errors identifying the directive.' \
    -m 'Cover ordinary messages, formatting round trips, slices, literal escaping and directive refusals through the public compiler API. Record the shared directive owner in the interpolation design read.'
