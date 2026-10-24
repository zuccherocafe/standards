#!/usr/bin/env bash
set -euo pipefail
solution_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
app_dir=${APP_DIR:-/app}
cd "$app_dir"
git apply "$solution_dir/reference.patch"
cargo fmt --all
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
if command -v design-read >/dev/null 2>&1; then
    design-read check
fi
git add Cargo.toml Cargo.lock clippy.toml src tests/scenarios.rs docs/design-reads/web-serving.md
git -c user.name='Evaluation' -c user.email='evaluation@example.invalid' commit \
    -m 'web: add bounded image transformations and content-addressed caching' \
    -m 'Parse image policy and query options into checked quantities and closed modes so invalid requests fail at the boundary. Resolve source content and the full output recipe into the cache identity, and keep original-file serving independent of transformation policy.'
