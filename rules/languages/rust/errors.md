# Rust standard: Errors

Part of the [Rust standard](README.md). This file covers who fixes an error, panics by crate kind, error types, refusals, and messages.

## Errors

### Who fixes it

*Default.* Decide an error's form by who can fix it.

- **The author or environment can fix it.** Return a typed `Err` at the boundary, with one variant per remedy, such as a bad authored file or model, a missing file, or a failed process.
- **A reader program returns malformed output.** Use one detail variant per module that reads it.
- **Accepted input fails numerically.** Return an outcome value.
- **Our own code breaks a promise.** Use a type that rules out the failure. Otherwise, follow the crate's panic policy.
- **A constant literal is invalid.** Use a panicking `const` constructor.

Before, an empty curve (our own broken promise) is carried as a refusal through every caller's `?`:

```rust
pub fn sample(&self, time: f64) -> Result<Pose, Error> {
    if right == 0 { return self.poses.first().copied().ok_or_else(|| invariant("empty curve")); }
```

After, the type cannot be empty, so sampling cannot fail:

```rust
pub(crate) struct Curve { keys: NonEmpty<Key> }
impl Curve {
    pub(crate) fn sample(&self, time: f64) -> Pose { /* first, last or between two keys */ }
}
```

**Checked by:** review.

### Panics by crate kind

*Always.*

| Crate | When an internal promise breaks | Lints |
|---|---|---|
| On a real-time or engine path (called every frame or from a script) | Do not panic intentionally. Handle every state and return a caller's mistake as a value. | `#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]` in `lib.rs`, plus `clippy::indexing_slicing`. |
| A vocabulary crate used on such a path | Never panic at run time. Return `Option` or `Result` from constructors, and panic in `const` constructors only during constant evaluation. | Same as the engine path, with `#[expect(clippy::panic, reason = "constant evaluation only")]` on each `const` constructor (`deny` allows `#[expect]`, whereas `forbid` would not). |
| Offline (a converter, build tool, or command-line program) | Call `expect("why it holds")`, and document it under `# Panics` on any public function. | Workspace `unwrap_used` warns; `expect_used` is allowed; enable `clippy::indexing_slicing` where indices come from external data until they use issued index types. |
| A binding's entry crate (contains only the binding's entry point) | Same as the engine path. | Same as the engine path. |

Test what the host does with a panic inside a binding call rather than assume the behaviour. State the result in the binding's crate documentation.

Ref: nextest's two-tier error model (`AGENTS.md`) and its many `expect` calls in nextest-runner; Andrew Gallant, ["Using unwrap() in Rust is Okay"](https://burntsushi.net/unwrap/).

**Checked by:** Clippy's `unwrap_used`, `expect_used`, `panic`, and `indexing_slicing` at the levels the table gives, and `todo`, `unimplemented`, and `unreachable`, for the constructs they recognise; `clippy.toml` exempts `unwrap_used`, `expect_used`, and `panic` in tests; `missing_panics_doc`; the binding's test of what the host does with a panic; review for the rest of the policy, including each `expect`'s reason, calls into functions that panic, and a `const` twin called only during constant evaluation.

### One error per operation

*Default.* Define one error type per boundary operation and name it after the operation, such as `ReadGlbError` or `BuildError`. Internal helpers return that operation's error, never a crate-wide error enum or `Result<Result<...>>`. When variants share context, wrap a `kind` enum in a struct. Each layer attaches the context it knows once: the core attaches the frame or position, its caller attaches the item, the shell attaches the file, and the parser attaches the line. Keep error types small by boxing large or rare causes (`clippy::result_large_err`), and return several independent refusals as one non-empty list.

Before, a refusal hidden in the success type:

```rust
fn find(&mut self, name: &str) -> Result<Result<PathBuf, String>, Error>
```

After:

```rust
fn find(&mut self, name: &str) -> Result<PathBuf, FindError>;

#[derive(Debug, thiserror::Error)]
pub enum FindError {
    #[error("{name} is not beside the model")]
    Missing { name: String },
    #[error(transparent)]
    File(FileError),
}
```

Ref: nextest's per-operation errors and `ConfigParseError { config_file, tool, kind }` (`nextest-runner/src/errors.rs`); ripgrep `ignore::Error::WithPath` (`crates/ignore/src/lib.rs`).

**Checked by:** Clippy's `result_large_err`; review.

### Where a refusal sits

*Always.* Place and move refusals as [A refusal sits where it is cheapest](../../general.md#design) specifies. When a change moves a refusal or improves its wording, the change lists each move and wording change. In projects that use error codes, keep each code stable unless the change explains why it changed.

**Checked by:** review of the change's list of moved refusals.

### Messages and codes

*Default.* Write each `Display` message in lowercase with no trailing period, phrased so it can follow `"failed to "` or `"{path}: "`. Return the underlying cause from `source()` instead of interpolating it into `Display`, so a cause is never both inline and a source and only the top-level renderer walks the source chain, once. Where a project uses error codes, put them on each binary's top-level error and on errors that non-Rust callers match, while Rust callers match typed variants on core errors. Never include an error code in `Display`: the renderer prints the code once, bindings to other languages return the code and message in separate fields, and nothing parses a code back out of formatted text.

```rust
#[derive(Debug, thiserror::Error)]
#[error("could not read or write {}", .path.display())]
pub struct FileError {
    pub path: PathBuf,
    #[source]
    pub source: io::Error,
}

// the binary's renderer
writeln!(errors, "error[{}]: {error}", error.code())?;
let mut cause = error.source();
while let Some(error) = cause {
    writeln!(errors, "  caused by: {error}")?;
    cause = error.source();
}
```

Ref: nextest's `AGENTS.md` on error messages and its `DisplayErrorChain` (`nextest-runner/src/errors.rs`); ripgrep prints the chain once (`crates/core/main.rs`).

**Checked by:** the report scans' code scan for codes in `#[error(...)]`; review.
