#!/usr/bin/env bash
set -euo pipefail
cd /app
solution=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# The upstream patch has tools/ before this fixture's workspace root.
git apply -p2 "$solution/feature.patch"

# Keep the reference within the workspace's line budget and name the formatted launch error.
python3 -I - <<'PY'
from pathlib import Path

path = Path('crates/dandegate/src/plan/template.rs')
text = path.read_text()
old = '    const NAMES: &\'static str = "{before}, {after}, {before-commit}, {after-commit}, {out}, {adapters}, {cache} and {input.<name>}";'
new = '''    const NAMES: &'static str = concat!(
        "{before}, {after}, {before-commit}, {after-commit}, ",
        "{out}, {adapters}, {cache} and {input.<name>}"
    );'''
assert text.count(old) == 1
path.write_text(text.replace(old, new))
path = Path('crates/dandegate/src/report.rs')
text = path.read_text()
old = '                Err(error) => Err(format!("the build caches cannot be locked: {error}")),'
new = '''                Err(error) => {
                    let why = format!("the build caches cannot be locked: {error}");
                    Err(why)
                }'''
assert text.count(old) == 1
path.write_text(text.replace(old, new))
PY
cp "$solution/dandegate-cache.md" docs/concord/dandegate-cache.md
cargo fmt --all
concord check
cargo test --offline --locked -p dandegate
git add crates/dandegate docs/concord/dandegate-cache.md
git commit --quiet \
    -m 'dandegate: give project steps a build cache that survives runs' \
    -m 'Fresh comparison checkouts make compilation start cold. Give each step and place a persistent build directory, separate from immutable result reuse and transient source checkouts.' \
    -m 'Take the cache lock before checkout and let supervisors and commands inherit ownership, so descendants of a killed report cannot race another writer. Contending reports use private scratch caches and explain the fallback. Bound persistent products at 32 GiB by evicting whole directories least recently used first while ownership is held.'
