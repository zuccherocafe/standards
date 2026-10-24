#!/usr/bin/env bash
set -euo pipefail
solution_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
app_dir="${APP_DIR:-/app}"
cd "$app_dir"
git apply "$solution_dir/reference.patch"
cargo fmt --all
cargo clippy --locked --offline --all-targets -- -D warnings
cargo test --locked --offline --workspace
design-read check
git add src tests docs README.md
git commit -m 'convert: attach rigid pieces after surface solvers' \
    -m 'Bind each dedicated piece to a closest usable cut triangle and retain its rigid rest offset. Refuse shared ownership and unusable support so a follower cannot deform or drive another owner.' \
    -m 'Place followers with the model skin after the spring pose, propagate their children and include attachment intent and compiled supports in the derived cache digest. Settle the skin read and exercise the public conversion boundary.'
