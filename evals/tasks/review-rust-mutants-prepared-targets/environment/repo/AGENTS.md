# Working in this rust-mutants slice

This workspace holds `rust-mutants` and its area design read under `docs/design-reads/rust-mutants.md`. Run checks directly from `/app` without `nix develop -c`:

    # Full checks
    cargo fmt --check && cargo clippy --offline --locked --all-targets -- -D warnings && cargo test --offline --locked

    # Quick checks
    cargo test --offline --locked
