# Working in this Soppo extraction

This crate holds Soppo's parser, type inference, Go code generator and source formatter in a single-source offline package. Run checks directly from `/app` without `nix develop -c`:

    # Full checks
    cargo fmt --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline

    # Quick checks
    cargo test --offline

`/app` is already an isolated scratch checkout on `main`; commit directly on the current branch without creating another clone or worktree.
