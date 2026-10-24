# Conservative enum control flow

The compiler slice checks integer return paths and nullable text access. Direct returns terminate; matches conservatively permit fallthrough. Its next change will recognise exhaustive returning matches and reuse their termination fact for nil narrowing. `Gate` has `Open` and `Closed` variants. Guards can be false, and a match with no selected arm falls through.

The core computes reports from source text. The CLI owns filesystem access. Scope is one name-resolved function, not a complete frontend or backend. Run `cargo test --offline --locked` and `cargo clippy --offline --locked --all-targets -- -D warnings`.
