# The Rust standard

This standard governs Rust in zucchero✿café projects. It applies [general.md](../../general.md) to Rust and adds Rust-specific rules. [review.md](../../review.md) turns both standards into review questions. [templates/rust/](../../../templates/rust/) contains the configuration for rules that tools can enforce.

References name paths in public repositories: [nextest](https://github.com/nextest-rs/nextest), [guppy](https://github.com/guppy-rs/guppy), [jiff](https://github.com/BurntSushi/jiff), [ripgrep](https://github.com/BurntSushi/ripgrep), and [rust-analyzer](https://github.com/rust-lang/rust-analyzer). When a rule is unclear, consult those codebases and the reasons for their designs. Each rule ends with a Checked by entry naming the compiler, a lint, a project check, a report, or review. Where a tool checks only part of a rule, the entry names that part. Examples from [tdollhouse](https://github.com/zuccherocafe/tdollhouse) appear where they teach something the references do not.

The standard is in several files. Read this one first, then each file whose subject a change touches:

- [Design](design.md): Types, parsing, where behaviour lives, visibility, records, modules and crates.
- [Errors](errors.md): Who fixes an error, panics by crate kind, error types, refusals and messages.
- [Tests](tests.md): The test policy, scenarios, expected values, properties and the mutation report.
- [Performance](performance.md): Hot paths, zero-cost checks, data layout, parallelism and measuring.
- [Lints and documentation](documentation.md): Lint exceptions, the report scans and documentation.
- [Async and concurrency](async.md): Only for a project that runs an async runtime.

## Toolchain and lints

Use edition 2024 and resolver 3. Each project pins a release or dated nightly in `rust-toolchain.toml`, with rustfmt and Clippy. The public API report needs rustdoc's unstable JSON output. Run it on a dated nightly with a `cargo-public-api` release that reads that nightly's format. Pin both together. The template uses that nightly as its toolchain. Lints live in `[workspace.lints]`, and every workspace member inherits them with `[lints] workspace = true`. The canonical table is [templates/rust/Cargo.toml](../../../templates/rust/Cargo.toml). Run Clippy as `cargo clippy --all-targets -- -D warnings`, so warnings fail the check. Forbid `unsafe_code` except in a crate whose job needs it, such as an engine binding's entry point. That crate documents the exception. rustfmt governs formatting. `cargo-deny` checks advisories, licences, duplicate versions, and dependency sources. Its advisory database may use a pinned, reviewed revision for reproducible offline checks. Keep the default `maximum-db-staleness`, so a snapshot older than ninety days fails. A vulnerability check needs new advisories to remain useful. Refresh the pin rather than raise the limit.

Declare every dependency once in `[workspace.dependencies]` as `major.minor`, such as `"0.23"` or `"2.14"`. Never specify a patch number or exact version, except for the exact-version exception below. Cargo treats a bare version as a caret requirement that accepts every compatible release. The committed `Cargo.lock` fixes the exact build version, and builds use `--locked`. A patch number in `Cargo.toml` therefore pins no build version and becomes stale. An exact requirement, such as `"=0.5.5"`, is allowed only when a release within the caret range is known to break the project. A comment above the requirement states what breaks. An engine binding whose generated API must match one engine release is the usual case. List the workspace's own crates first, by path, followed by a blank line and external crates. Members use `workspace = true` for workspace dependencies and add only features. Use the same grouping in members: workspace crates, a blank line, and external crates.

**Checked by:** rustc, rustfmt, and Clippy with `-D warnings` over the template's lint table, and `cargo-deny`, in the project's checks; `--locked` builds for the lockfile; review for how dependencies are declared.

## For a new project: not yet decided

A project answers these open questions in its `AGENTS.md` when it needs the relevant capability, with prior art. These are not rules yet. A project without the need, such as network access or async, forbids the capability rather than leaving it unspecified.

- **Network and TLS.** Decide which crates may be network clients, using a per-crate allow-list checked against the dependency graph.
- **Tracing and redaction.** Define structured spans per operation and log levels. Address types that contain secrets or private conversations and redact them in `Debug`. Address human diagnostics on stderr and machine output on stdout.
- **Layered configuration.** Define precedence and provenance for files, environment variables, and flags. Address one-time parsing into typed state with `deny_unknown_fields`, secrets, and reload semantics. Prior art includes `nextest-runner/src/cargo_config/discovery.rs`.
- **Machine-readable command-line output.** Define semantic exit statuses, broken-pipe handling, stdout and stderr separation, and a versioned machine format. nextest-metadata provides prior art.
- **Migrations of durable data.** Data that cannot be rebuilt from sources needs schema versions, migrations, atomic and durable writes, concurrent writers, and recovery. Derived records can instead refuse an unknown version and request a rebuild. Prior art includes nextest's old-reader and new-data compatibility matrix (`AGENTS.md`).
- **Protocol stability.** A protocol with outside peers needs `#[non_exhaustive]`, versions, and compatibility tests. A published crate needs a policy for SemVer, its minimum supported Rust version, and features.
- **Wall-clock time.** Distinguish timestamps and time zones, as in jiff, from durations.
