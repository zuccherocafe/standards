# Offline conversion workspace

This workspace isolates the published cut mesh, its skin and the spring-driven offline pose. Work on the current branch. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --locked --offline --workspace` from this directory.

Keep the genuine solver and vocabulary in `crates/` intact; attachment behaviour belongs in the converter crate. Runtime engine bindings, model import, furniture authoring, network access and async are outside this workspace. Coordinates and tolerances use metres and f64. The retained skeleton is parent-first, and skin joint slots are local to each surface.
