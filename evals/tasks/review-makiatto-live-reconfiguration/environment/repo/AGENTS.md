# Working in this Makiatto service slice

This crate extracts Makiatto's listener reload, routing weights and WebAssembly transform cache. Run checks directly from `/app` without `nix develop -c`:

    # Full checks
    cargo fmt --check && cargo clippy --offline --locked --all-targets -- -D warnings && cargo test --offline --locked

    # Quick checks
    cargo test --offline --locked
