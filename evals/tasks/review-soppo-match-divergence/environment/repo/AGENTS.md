# Working in this Soppo inference slice

This crate holds Soppo's pattern-match control-flow analysis, missing-return checks and nil-safety narrowing. Run checks directly from `/app` without `nix develop -c`:

    # Full checks
    cargo fmt --check && cargo clippy --offline --locked --all-targets -- -D warnings && cargo test --offline --locked

    # Quick checks
    cargo test --offline --locked
