# Working in this dandegate extraction

This workspace holds `dandegate` and its sibling workspace tools in an offline checkout. Run checks directly from `/app` without `nix develop -c`:

    # Full checks
    cargo fmt --check && cargo clippy --offline --locked --all-targets -- -D warnings && cargo test --offline --locked -p dandegate

    # Quick checks
    cargo test --offline --locked -p dandegate --lib

`/app` is already an isolated scratch checkout on `main`; commit directly on the current branch without creating another clone or worktree.
