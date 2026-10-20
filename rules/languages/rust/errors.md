# Rust standard: Errors

Part of the [Rust standard](README.md). Who fixes an error, panics by crate kind, error types, refusals and messages.

## Errors

### Who fixes it

*Default.* Decide an error's form by who can fix it.

- **The author or environment can fix it.** Return a typed `Err` at the boundary, with one variant per remedy. Cases include an authored file, a model, a missing file, or a failed process.
- **A reader program returns malformed output.** Use one detail variant per module that reads it.
- **Accepted input fails numerically.** Return an outcome value.
- **Our own code breaks a promise.** Use a type that rules out the failure. Otherwise, follow the crate's panic policy.
- **A literal is invalid.** Use the `const` counterpart.

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

| Crate | A broken internal promise | Lints |
|---|---|---|
| On a real-time or engine path (called every frame or from a script) | no intentional panic: every state handled, a caller's mistake returned as a value | `#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]` in `lib.rs`; `indexing_slicing` on |
| A vocabulary crate used on such a path | never panics at run time; constructors return `Option` or `Result`; `const` twins panic only in constant evaluation | as the engine path, each `const` twin under `#[expect(clippy::panic, reason = "constant evaluation only")]` (a crate-level `deny` yields to it; `forbid` would not) |
| Offline: a converter, a build tool, a command-line program | `expect("why it holds")`, with `# Panics` on a public function | workspace `unwrap_used` warns; `expect_used` allowed; `indexing_slicing` where indices come from outside data until those indices are issued types |
| A binding's entry crate | nothing but the binding's entry point | as the engine path |

Test what the host does with a panic inside a binding call rather than assume the behaviour. State the result in the binding's crate documentation.

Ref: nextest's two-tier error model (`AGENTS.md`) and its many `expect` calls in nextest-runner; Andrew Gallant, ["Using unwrap() in Rust is Okay"](https://burntsushi.net/unwrap/).

**Checked by:** Clippy's `unwrap_used`, `expect_used`, `panic`, and `indexing_slicing` at the levels the table gives, and `todo`, `unimplemented`, and `unreachable`, for the constructs they recognise; `clippy.toml` exempts `unwrap_used`, `expect_used`, and `panic` in tests; `missing_panics_doc`; the binding's test of what the host does with a panic; review for the rest of the policy, including each `expect`'s reason, calls into functions that panic, and a `const` twin called only during constant evaluation.

### One error per operation

*Default.* Use one error type per operation a caller performs across a boundary. The layer that knows the context attaches it once. Name the error after the operation, such as `ReadGlbError` or `BuildError`. Internal helpers return their operation's error, never a crate-wide enum or `Result<Result<…>>`. Shared context is a struct around a `kind`. A core attaches the frame or position it knows. Its caller attaches the item, the shell attaches the file, and the parser attaches the line. Keep each error small. Box a large or rare cause (`clippy::result_large_err`). Report several independent refusals as one non-empty list.

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

*Always.* A refusal sits and moves as [A refusal sits where it is cheapest](../../general.md#design) says. A change's description lists every move. A refusal's wording may improve for its reader, and the change lists that improvement too. Where a project has codes, a code stays stable unless the change explains why it changes.

**Checked by:** review of the change's list of moved refusals.

### Messages and codes

*Default.* `Display` supplies its own message fragment. Put the cause in `source()`. A message is lowercase with no trailing period and reads after "failed to" or "{path}: ". A cause is never both inline and a source. Only the renderer walks the chain, once. Where a project has error codes, they belong to each binary's top error and to errors matched by non-Rust readers. Core errors carry typed variants for Rust callers to match. Never put a code inside `Display`. The renderer prints it once, and nothing reads a code back from text. A binding to another language returns code and message in separate fields.

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
