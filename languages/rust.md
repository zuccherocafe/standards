# The Rust standard

This standard governs Rust in zucchero✿café projects. It applies [general.md](../general.md) to Rust and adds Rust-specific rules. [review.md](../review.md) turns both standards into review questions. [templates/rust/](../templates/rust/) contains the configuration for rules that tools can enforce.

References name paths in public repositories: [nextest](https://github.com/nextest-rs/nextest), [guppy](https://github.com/guppy-rs/guppy), [jiff](https://github.com/BurntSushi/jiff), [ripgrep](https://github.com/BurntSushi/ripgrep), and [rust-analyzer](https://github.com/rust-lang/rust-analyzer). When a rule is unclear, consult those codebases and the reasons for their designs. Examples from [tdollhouse](https://github.com/zuccherocafe/tdollhouse) appear where they teach something the references do not.

## Toolchain and lints

Use edition 2024 and resolver 3. Each project pins a release or dated nightly in `rust-toolchain.toml`, with rustfmt and Clippy. The public API report needs rustdoc's unstable JSON output. Run it on a dated nightly with a `cargo-public-api` release that reads that nightly's format. Pin both together. The template uses that nightly as its toolchain. Lints live in `[workspace.lints]`, and every workspace member inherits them with `[lints] workspace = true`. The canonical table is [templates/rust/Cargo.toml](../templates/rust/Cargo.toml). Run Clippy as `cargo clippy --all-targets -- -D warnings`, so warnings fail the check. Forbid `unsafe_code` except in a crate whose job needs it, such as an engine binding's entry point. That crate documents the exception. rustfmt governs formatting. `cargo-deny` checks advisories, licences, duplicate versions, and dependency sources. Its advisory database may use a pinned, reviewed revision for reproducible offline checks. Keep the default `maximum-db-staleness`, so a snapshot older than ninety days fails. A vulnerability check needs new advisories to remain useful. Refresh the pin rather than raise the limit.

Declare every dependency once in `[workspace.dependencies]` as `major.minor`, such as `"0.23"` or `"2.14"`. Never specify a patch number or exact version, except for the exact-version exception below. Cargo treats a bare version as a caret requirement that accepts every compatible release. The committed `Cargo.lock` fixes the exact build version, and builds use `--locked`. A patch number in `Cargo.toml` therefore pins no build version and becomes stale. An exact requirement, such as `"=0.5.5"`, is allowed only when a release within the caret range is known to break the project. A comment above the requirement states what breaks. An engine binding whose generated API must match one engine release is the usual case. List the workspace's own crates first, by path, followed by a blank line and external crates. Members use `workspace = true` for workspace dependencies and add only features. Use the same grouping in members: workspace crates, a blank line, and external crates.

## Types and invariants

### Identities

An identity in a crate-level interface, or one that is stored, has its own type. A crate-level interface is a `pub(crate)` or wider signature, a record, or an error. Use a newtype with a private field, a checked constructor, an accessor, `Display`, and `Eq + Ord + Hash`. Key maps by the identity type so a key from one map does not fit another. When authored and stored names differ, the type has two entry points. One accepts the authored form. `FromStr` accepts the stored form, which must round-trip. For example, a person may write "violin case" while the program stores `violin_case`.

Before, the maps share a string key type:

```rust
pub parts: IndexMap<String, Part>,
pub choices: IndexMap<String, Choice>,
```

After:

```rust
/// A bone, as the model names it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoneName(Box<str>);

impl BoneName {
    /// # Errors
    /// [`EmptyName`] for an empty name.
    pub fn new(name: impl Into<Box<str>>) -> Result<Self, EmptyName> { /* refuse "" */ }
    pub fn as_str(&self) -> &str { &self.0 }
}

pub parts: IndexMap<PartId, Part>,
pub choices: IndexMap<ChoiceId, Choice>,
```

Ref: nextest `RustBinaryId` (`nextest-metadata/src/test_list.rs`); guppy `PackageId` (`guppy/src/package_id.rs`).

### Quantities

A quantity has a type. A unit is never merely a name. A length, angle, time, or rate in a crate-level interface uses a unit type. Examples include `Length`, `Radians`, `Seconds`, and `Rate`. Use one explicit conversion where another unit enters from an authored file. A unitless type is only for a dimensionless value. A compound dimension read by one consumer keeps its unit in its name rather than gaining a one-use type. A record keeps its wire spelling with `#[serde(rename = "margin_metres")]`.

Type crate-level interfaces and stored state, not arithmetic. A numerical kernel is a private module with one caller in its crate. Its functions may take and return primitive types and slices, unwrapped once at entry and wrapped once at exit. A unit type has only operations its callers use. Every `impl Mul<Seconds> for Rate` is a dimensional claim. The exception is temporary numerical arithmetic. A local or intermediate calculation result, and a kernel argument, remain the primitive types the arithmetic uses. Stored state and values protected by private invariants keep their types wherever they occur. [Identities](#identities) defines a crate-level interface.

Ref: jiff `SignedDuration` (`crates/jiff/src/signed_duration.rs`); nextest stores time only as `Duration` (`nextest-runner/src/config/elements/slow_timeout.rs`); jiff moved ranged integers out of its internals and kept them at the public boundary (`CHANGELOG.md`, 0.2.20).

### Invariants

Establish an invariant once and preserve it on every path. An invariant type has private fields in its owning module and one checked constructor, `new(raw) -> Result<Self, ItsOwnError>`. Use `Option` instead when there is one reason to refuse. Every other construction or mutation preserves the invariant. `Deserialize` calls the constructor. `Default` exists only if the default is valid. Mutators and `From` conversions preserve the invariant. `pub(crate)` fields permit construction throughout the crate and are not private. A `const` counterpart that panics exists only where a constant needs it. It panics only during constant evaluation. Use no `new_unchecked` unless a measurement shows that checking has a cost. A function that checks a value returns the checked type, not `()`, so callers retain proof of the check.

Before:

```rust
fn identifier(name: &Located<String>) -> Result<(), Error> { /* checks the grammar; the caller keeps a String */ }
```

After:

```rust
/// A runtime identity: lowercase letters, digits and underscores, starting with a letter.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RuntimeId(Box<str>);

impl RuntimeId {
    /// # Errors
    /// [`InvalidRuntimeId`] for an empty name or a character outside the grammar.
    pub fn new(name: &str) -> Result<Self, InvalidRuntimeId> { /* the same grammar */ }
}
```

Ref: nextest `TestPriority::new` and its `Deserialize` (`nextest-runner/src/config/elements/priority.rs`); nextest `ErrorList::new` returns `Option`, so no empty list exists (`nextest-runner/src/errors.rs`).

### The numerical contract

Valid inputs do not guarantee a valid result.

- Give a combination of individually valid values that an algorithm cannot use its own named outcome. Never narrow a general type to fit one algorithm.
- Finite inputs do not guarantee finite arithmetic. Check a result that can overflow or lose definition where it is produced. Alternatively, document an input bound that proves the result finite.
- Sizes and indices use `try_from` and either `checked_*` or `saturating_*`. Document the chosen meaning. Use `as` only under `#[expect(clippy::cast_*, reason = "...")]` that names the bound.
- A tolerance is a named constant beside the type that uses it.
- Public components do not enforce an invariant. A `glam` quaternion is not unit by construction, and multiplication does not renormalise it.
- Numerical failure of accepted input is an ordinary outcome value, distinct from a refusal of an author's mistake and from a bug.
- `debug_assert!` documents an expectation. It proves nothing in a release build.

Ref: jiff checks its arithmetic before using its unchecked constructor (`crates/jiff/src/signed_duration.rs`).

### Closed sets

A closed set is an enum. Match a crate's own enums exhaustively. Put case-dependent behaviour in a method on the enum. Where a new variant would need a decision, use neither an `_` arm nor `matches!` on the crate's own enum. Adding that variant then fails to compile at the relevant use.

Ref: guppy `DependencyDirection::opposite` (`guppy/src/graph/mod.rs`); nextest `ExecutionResult::as_static_str` (`nextest-runner/src/reporter/events.rs`).

### Sum types

State that varies together, or may be absent, is a sum type. Replace a `bool` that selects a mode or determines other fields' meanings with an enum. Each variant contains exactly the fields for its case. Represent options set together, an `Option` tied to one value of a neighbouring enum, or parallel vectors that must agree as one variant or one vector of structs. Absence is `Option` or a named variant, never an empty string, infinity, `-1`, or zero. A `bool` that is an independent observation stays a `bool`. Before combining two flags into one enum, prove from the code that they cannot both hold. Convert a command-line or file flag to its enum at the boundary.

Before, what `end` means depends on `valid`, and a third case has no flag at all:

```rust
pub struct Cell {
    /// The lowest clear point; the far end where the line is blocked, zero where it misses.
    pub end: f64,
    pub valid: bool,
    pub gate: f64,
}
```

After:

```rust
pub(crate) enum Cell {
    Missed,
    Blocked { far_end: f64 },
    Clear { lowest: f64, gate: f64 },
}
```

Ref: nextest `MaxFail::from_fail_fast(bool)` turning a flag into its enum (`nextest-runner/src/config/elements/max_fail.rs`); `RetryPolicy::{Fixed, Exponential}` (`retry_policy.rs`); `terminate_after: Option<NonZeroUsize>` (`slow_timeout.rs`).

### Tuples

A domain value or interface value with its own meaning is a named type, not a bare tuple. In a crate-level interface or a stored field, same-typed values whose order carries meaning use named struct fields or a [small key set](#small-key-sets). Examples include low and high values or left and right values. A tuple would allow a caller to swap them. A numerical kernel's working pairs remain tuples under the [Quantities](#quantities) exception, such as bracket endpoints passed between its functions. Different-typed pairs that the caller unpacks at once also remain tuples. Their types already distinguish the values, so a one-use struct adds no useful name.

Before, only a convention says which end is the root:

```rust
fn ends(&self) -> (DVec3, DVec3)
```

After:

```rust
pub(crate) struct Ends { pub(crate) root: DVec3, pub(crate) tip: DVec3 }
fn ends(&self) -> Ends

// Kept: two types, unpacked where they are returned.
let (clip, notices): (Clip, Vec<Notice>) = read(&take)?;
```

### Small key sets

Represent one value per member of a small closed key set with a structure indexed by the key's enum. Never use an array with an order convention or an integer key. For example, in tdollhouse: `PerSide<T>`, indexed by `Side`, stores one value per body side. Never use `[T; 2]` with a "left first" convention or a `usize` side. With an invalid integer side, `1 - side` underflows.

```rust
pub enum Side { Left, Right }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerSide<T> { pub left: T, pub right: T }
impl<T> Index<Side> for PerSide<T> { /* match side */ }
```

`PerSide` proves that there are exactly two values and that a side is one of two enum values. It does not prove that the stored values belong to the right owner.

### Index spaces

Each index space has its own index type, issued by its collection. Only the collection's crate can construct an index from an offset, through a sealed trait or crate-private constructor. The collection issues indices through operations such as `push` or iteration. A typed index proves its index space, not its originating instance. An index from one solver is a valid index for another. Where a handle might come from another instance, bind it to its source or check it once at entry.

Ref: guppy's private `PackageIx(u32)` (`guppy/src/graph/mod.rs`); rust-analyzer's `la-arena` `Idx<T>`.

### Checked against a source

A value checked against a source retains that source. A value valid only with another value, such as a clip checked against one body, holds a reference to that source. Construct the checked value through its source. No operation then takes the pair separately or can combine mismatched values. The lifetime prevents the checked value from outliving its source but does not distinguish source instances. Lifetimes are not brands, so construction must go through the source. A stage gets its own type when it establishes a fact that the next operation needs. An ordinary transformation that establishes nothing new does not.

For example, in tdollhouse, before:

```rust
pub fn solve(body: &Body, clip: &Clip<'_>, settings: &Settings) -> Result<Fit, Error> // refuses a mismatched pair
```

After:

```rust
impl Body {
    /// # Errors
    /// [`PoseError`] for a frame this body cannot place.
    pub fn clip<'b>(&'b self, rate: Rate, frames: impl IntoIterator<Item = Joints>) -> Result<CheckedClip<'b>, PoseError>;
}
pub struct CheckedClip<'b> { body: &'b Body, rate: Rate, poses: Vec<Pose> }

/// Infallible: numerical failure is an outcome inside the result.
pub fn solve(clip: &CheckedClip<'_>, settings: &Settings) -> Fit;
```

Ref: guppy `PackageMetadata<'g>` borrowing its graph (`guppy/src/graph/graph_impl.rs`); nextest `EarlyProfile` consumed into `EvaluatableProfile` (`nextest-runner/src/config/core/imp.rs`).

### Lifetimes

A type carries a lifetime only when it represents a borrowed view. The borrow either binds the value to its source or avoids copying large data the caller still needs. Merely bundling borrowed arguments does not justify a struct. Pass the arguments or make the operation a method on the data's owner.

## Parsing at boundaries

### Parse once

This section applies `general.md`'s boundary rule to Rust. Parse an authored file, a reader's output, a record read back, or a script-supplied value into domain types at its boundary. Refusals there name the file, line, and field. Past the boundary, no function takes the raw form. In crates that take paths, convert paths to UTF-8 once at entry, using `camino`'s `Utf8PathBuf`.

Ref: Alexis King, ["Parse, don't validate"](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/); rust-analyzer builds `CrateGraph` through constructing functions (`docs/book/src/contributing/architecture.md`).

### Deserialising

Deserialisation calls the constructor. An invariant type uses `#[serde(try_from = "Raw")]` or a hand-written `Deserialize` that calls `new`. Its error has its own type, not `String`. Never use `#[serde(untagged)]`. Write a visitor with an `expecting` message. Records read back use `deny_unknown_fields`.

Ref: nextest `ConfigIdentifier`'s `Deserialize`, tested through both entry paths (`nextest-runner/src/config/core/identifier.rs`).

## Where behaviour lives

### Methods on the owner

Behaviour that reads one type's data is a method on that type. Pure maths over plain values remains a free function. A free function implements an algorithm over peer inputs with no owner, such as distance between shapes or a spline, or is a private helper.

### Standard traits

Use the standard trait that fits the operation. Use `Display` for human-readable values and `FromStr` for stored text syntax. Use `From` and `TryFrom` for boundary conversions, and `Index` for keyed access. Use `IntoIterator` and `Iterator` for traversal. Use `Deref` only for a collection newtype.

Ref: nextest `TestThreads` with `FromStr`, `Display`, and a visitor (`nextest-runner/src/config/elements/test_threads.rs`).

### Traits

Do not introduce a trait merely for one implementation. A trait exists where several types implement it or where it inverts a dependency a core must not have. In the latter case, the core declares the trait and hosts implement it without adding the host dependency to the core. An adaptor trait for foreign types at a named boundary is allowed. A test double alone does not justify a trait. When a test needs made-up inputs, put the algorithm in a plain function over values. Both the production type and the test call that function. Production behaviour remains an inherent method.

Ref: nextest `StoreReader` with two implementations (`nextest-runner/src/record/reader.rs`); guppy's `IntoMietteDiagnostic` adapting foreign errors (`target-spec-miette/src/imp.rs`).

## Visibility

The public API is what other crates use. An item is `pub` only when another crate names it. A module is `pub` only to group such items. Everything else is `pub(crate)` or narrower, enforced by rustc's `unreachable_pub`. A field is public only when every value of its type is valid there. A struct has either all public fields or none, enforced by Clippy's `partial_pub_fields`. A crate exports types and operations, not internal tuning. Caller-variable settings are fields of a settings type with a `const DEFAULT`. Tuning that another crate must read is exported read-only and accepted by no function. A sealed trait may hide a constructor. `#[non_exhaustive]` follows a compatibility promise. A workspace whose crates are not published makes no such promise. The public API report, [templates/rust/tools/api.py](../templates/rust/tools/api.py), lists each library crate's public items before and after a change. Reviewers read these promises before the implementation. The report causes no check failures.

Ref: nextest's `AGENTS.md` ("Use `pub(crate)` and `pub(super)` liberally"); nextest's raw serde shapes are `pub(in crate::config)` (`nextest-runner/src/config/overrides/imp.rs`).

## Records and serialisation

### One type per representation

A concept has one type for each genuinely different representation, such as a persistent name or an index bound to a live collection. Never add a copy, a differently structured duplicate, or a parallel string table. An opaque name needed by several crates lives in their lowest shared crate. An index lives with its issuing collection. A type used by one crate lives there. A conversion between representations is explicit, named, and checked where it can fail.

Ref: nextest-filtering imports `RustBinaryId` from nextest-metadata (`nextest-filtering/src/expression.rs`); rust-analyzer chooses boundary counterparts deliberately (`docs/book/src/contributing/architecture.md`).

### Records

A record serialises the core's report type or one wire type with one conversion. Core report types derive `Serialize` and `Deserialize` behind an optional `serde` feature enabled by the writing crate. Working inputs never do. A separate wire type exists only when the wire form differs, such as names instead of indices, a projection, or a legacy shape. It has exactly one conversion and a round-trip test against a hand-written fixture file. A change to a written file's bytes states the change and names the file and format tag. It updates every reader in the same commit. Disk collections are ordered, using `BTreeMap` or `IndexMap`. `HashMap` and `HashSet` serve lookups only.

Ref: guppy serialises `guppy-summaries`, not `PackageGraph`; nextest serialises `nextest-metadata` summaries, not its test list; ripgrep's `grep-printer` keeps serde behind a feature (`crates/printer/Cargo.toml`).

### Debug

`Debug` is for diagnostics. It is never a stable encoding, record field, identity, or cache-key input. A record names a case through serde, `Display`, or an `as_str` with one arm per variant. Diagnostic `Debug` output in logs and test failures is allowed.

Before, renaming a variant silently invalidates every cache:

```rust
.map(|(role, named)| (format!("{role:?}"), named.bone.as_str()))
```

After: the role's serde spelling, or an `as_str` with one arm per variant.

## Errors

### Who fixes it

Decide an error's form by who can fix it.

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

### Panics by crate kind

| Crate | A broken internal promise | Lints |
|---|---|---|
| On a real-time or engine path (called every frame or from a script) | no intentional panic: every state handled, a caller's mistake returned as a value | `#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]` in `lib.rs`; `indexing_slicing` on |
| A vocabulary crate used on such a path | never panics at run time; constructors return `Option` or `Result`; `const` twins panic only in constant evaluation | as the engine path, each `const` twin under `#[expect(clippy::panic, reason = "constant evaluation only")]` (a crate-level `deny` yields to it; `forbid` would not) |
| Offline: a converter, a build tool, a command-line program | `expect("why it holds")`, with `# Panics` on a public function | workspace `unwrap_used` warns; `expect_used` allowed; `indexing_slicing` where indices come from outside data until those indices are issued types |
| A binding's entry crate | nothing but the binding's entry point | as the engine path |

Test what the host does with a panic inside a binding call rather than assume the behaviour. State the result in the binding's crate documentation.

Ref: nextest's two-tier error model (`AGENTS.md`) and its many `expect` calls in nextest-runner; Andrew Gallant, ["Using unwrap() in Rust is Okay"](https://burntsushi.net/unwrap/).

### One error per operation

Use one error type per operation a caller performs across a boundary. The layer that knows the context attaches it once. Name the error after the operation, such as `ReadGlbError` or `BuildError`. Internal helpers return their operation's error, never a crate-wide enum or `Result<Result<…>>`. Shared context is a struct around a `kind`. A core attaches the frame or position it knows. Its caller attaches the item, the shell attaches the file, and the parser attaches the line. Keep each error small. Box a large or rare cause (`clippy::result_large_err`). Report several independent refusals as one non-empty list.

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

### Where a refusal sits

Place a refusal where it is cheapest and clearest for the author. Refuse as soon as the needed facts are known, before expensive work the refusal would waste. The order of independent refusals is not a promise. A change may move a refusal earlier or later to make it cheaper or clearer. Its description lists every move. No test fixes which independent fault is reported first. Never let a fault pass unrefused or make a refusal unreachable. A refusal's wording may improve for its reader, and the change lists that improvement too. Where a project has codes, a code stays stable unless the change explains why it changes.

### Messages and codes

`Display` supplies its own message fragment. Put the cause in `source()`. A message is lowercase with no trailing period and reads after "failed to" or "{path}: ". A cause is never both inline and a source. Only the renderer walks the chain, once. Where a project has error codes, they belong to each binary's top error and to errors matched by non-Rust readers. Core errors carry typed variants for Rust callers to match. Never put a code inside `Display`. The renderer prints it once, and nothing reads a code back from text. A binding to another language returns code and message in separate fields.

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

## Modules, crates, and purity

### Modules

Keep one cohesive concept per module. Cooperating types may share a file, and a large concept may occupy one large file. A parent may contain a façade's entry functions, declarations, and re-exports, so splitting a file never changes a caller's path. Merge files too small to stand alone into their owner. Group flat directories by subject. A module directory is `name/` beside `name.rs`, never `mod.rs`, as `clippy::mod_module_files` enforces. Tests sit beside their subject in `#[cfg(test)] mod tests`. They may use a named test module when shared fixtures make that more cohesive. Scenario tests live in the crate's `tests/`. Impose no file-count or one-type-per-file rules.

Ref: nextest `config/elements/`, one element per file with its impls and tests; guppy's `graph_impl.rs` holds several cooperating types.

### Crates

A crate exists for a reason the compiler or build can check. Such reasons include a different licence or artefact, a confined heavy or platform dependency, an absence checked by the crate graph, or independent use by another crate. An artefact may be a host library, a binary, or a binary that nothing may link. Size, a pipeline stage, or a group of helpers does not justify a crate. Use modules for those. A crate directory takes its crate's name. Each package's `Cargo.toml` has a one-sentence `description` of the crate's purpose, agreeing with its first `//!` line.

Ref: matklad, ["Fast Rust Builds"](https://matklad.github.io/2021/09/04/fast-rust-builds.html); ripgrep's `grep-matcher` depends only on `memchr`.

### Purity

Cores compute. The shell does input and output. An engine-independent core reads no file, clock, environment variable, or process and spawns nothing. Purity applies at crate level, and the dependency list is the cheapest proof. Purity alone does not justify a crate because `std::fs` is in every crate's graph. Inside an input-output crate, purity applies at function level. Each stage's logic is a named function over values, tested on values. Named modules perform input and output at the boundaries. Document a clock read that only reports timings and changes no output in the crate's documentation.

Ref: rust-analyzer's core "never does any IO" (`docs/book/src/contributing/architecture.md`); nextest-runner does IO throughout and keeps pure functions such as `compute_outstanding_pure` (`nextest-runner/src/record/rerun.rs`).

## Performance

### Hot paths

A crate that runs per frame or per item states that fact, its budget, and its measurement method in its crate documentation. Once warm, per-frame and per-item code allocates nothing. Reuse buffers owned by the caller or by a scratch structure the caller retains. Test this promise with a counting global allocator in a dedicated integration-test binary, so no other test uses the allocator. Obtain the allocator from a small development dependency, so the test needs no `unsafe`. Size collections once from known information with `with_capacity` or `reserve`, rather than growing them per item.

Ref: rust-analyzer's "Avoid Allocations", "Push Allocations to the Call Site" and "Avoid Intermediate Collections" (`docs/book/src/contributing/style.md`).

### Zero-cost abstractions, verified

Check rather than assume that newtypes, small `Copy` types, and checked constructors cost nothing in release builds. A scalar newtype is a single-field `Copy` struct. Add `#[repr(transparent)]` only for a layout promise, such as FFI, `bytemuck`, or transmutes, never for speed. Small cross-crate accessors and conversions rely on Rust's inlining of tiny functions. Add `#[inline]` when a profile or `nm` on the release binary shows a hot function left out of line. Include the measurement in the commit message. A change that adds types to a hot path reports its instruction count before and after.

### Copies and data layout

Pass large values by reference and small `Copy` values by value. Never clone merely to satisfy the borrow checker in a hot path. Use contiguous per-item vectors with typed indices ([Index spaces](#index-spaces)), rather than maps or pointer graphs. In a core's inner loop, prefer `f64` arithmetic on plain values. Convert to unit types at the function boundary, checking each value once ([Quantities](#quantities)). Avoid generic code over many types at crate boundaries, as rust-analyzer's "Avoid Monomorphization" advises. Monomorphised copies cost compile time and instruction cache.

### Parallelism

Parallelise independent items in the largest groups that keep every core busy. Return results in item order, regardless of thread execution order. Use `std::thread::scope` and an atomic cursor, without a runtime. Document the group size and the wall-time bound, which is the largest item, in the crate documentation. Core results never depend on thread count. Obtain the thread count from `available_parallelism`. It honours CPU affinity, so a pinned run measures the selected CPUs.

### Measuring

Measure only release builds with the workspace's own profile. To compare revisions, report instructions retired with `perf stat -e instructions:u`. For budgets, report CPU and wall time with the median, range, and run count. For builds, report peak memory. Profiles use frame pointers (`-C force-frame-pointers=yes`) and line tables in a separate target directory, leaving the measured binary unchanged. Benchmark harnesses are development dependencies of the measured crate, admitted like other test-only dependencies. Prefer divan, or criterion where the project already uses it. Benchmark harnesses are never part of the project's checks.

Ref: jiff keeps its Criterion suite in a separate `bench/` crate; ripgrep's `benchsuite/` records runs per machine and date; rust-analyzer's `xtask metrics` records build and analysis times rather than failing on them.

## Tests

### The test policy

A crate's tests come in three layers, written in this order, and a review reads them in the same order.

1. **Scenarios.** Write one test per use through the public entry point where the crate is used, most common use first. Examples include a command-line build, an import, a played clip, or an unchanged-input rebuild that writes nothing new.
2. **Function promises.** Write unit tests of each function's promises beside that function, using small made-up inputs.
3. **Properties.** Test invariants that types cannot express, using values generated from the valid domain or malformed input at a boundary.

Write expected values from the input's source, a specification, or a law, never from a run. A reviewer-accepted snapshot of exact text or structured output whose shape is the promise also counts as authored ([Expected values and snapshots](#expected-values-and-snapshots)). Keep a test that replays a review finding only when real data produces its input. Name it for its promise. For an input nothing real produces, answer the finding with the type or reasoning rather than a test. A scenario in the checks costs seconds. Use the smallest input that reaches the path, such as a code-built character or a fixture with a few files. A minutes-long run over real data belongs in the change report, not the checks. The report shows what changed.

### Scenarios first

Each crate's `tests/scenarios.rs` begins with module documentation listing the crate's uses, most common first. Each use has an obvious test through its actual entry boundary. Examples include a command-line build or a solve on made-up input through the public entry point. A small constructor or kernel needs no committed fixture.

For example, in tdollhouse: the first scenario covers a spring chain trailing a moving parent and settling back, its most common behaviour. Set thresholds where a person would see incorrect behaviour, never at today's values.

```rust
/// Hair behind a turning head trails it, swings, and settles where the clip points it.
#[test]
fn a_chain_trails_a_turning_parent_then_settles_where_the_clip_points_it() {
    // …
    // A spring that never lags reads as rigid; a degree is what a person notices.
    assert!(lag > 1.0_f64.to_radians());
}
```

Ref: nextest's integration tests drive the real binary on a fixture project; ripgrep's first tests are the commands people type (`tests/misc.rs`).

### Expected values and snapshots

Expected numbers and behaviour are written by hand from the fixture's source, a specification, or a law, never copied from a run. Snapshots with `insta` cover exact text or structured output whose shape is the promise, such as help text, a rendered error, or a listing. A reviewer-accepted snapshot in the diff (`cargo insta review`) is an authored expectation. Never snapshot numbers or behaviour. Keep hand-written expectations and properties for them. The project's change report shows differences.

```rust
// Authored from the fixture's own files: it holds these four animations.
assert_eq!(manifest.clip_names(), BTreeSet::from(["Nod", "Rest", "Stride", "Turn"]));

// Exact text whose wording is the promise.
insta::assert_snapshot!(render(&check(&fixture().with(r#"model "model.obj""#))));
```

Ref: nextest's integration snapshots of help, errors, and diagnostics (`integration-tests/tests/integration/snapshots/`); nextest's hand-written fixture model (`fixture-data/src/fixture_project.rs`).

### Properties

Properties test what types cannot express. Generate valid-domain values through domain constructors. Generate malformed input, such as arbitrary text or numbers, at a parsing boundary to test refusal. Prefer an oracle, then a round trip, then an invariance promised by the specification. Examples include stable output for the same input, independence from unit order, or preservation of already-clear values. Never test a property the code does not promise. Property tests in the checks use a fixed seed and case budget so rerunning the test reproduces a failure. They write nothing. Turn off proptest's default persistence in every configuration because it writes a regressions file beside the test. Explore fresh seeds by hand. Keep a counterexample to a real promise as an ordinary unit test named for that promise.

```rust
proptest! {
    #![proptest_config(Config {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x7d0c),
        failure_persistence: None,
        ..Config::default()
    })]
```

Ref: jiff's `Arbitrary for Date` through its constructor (`crates/jiff/src/civil/date.rs`); nextest's `sut_matches_oracle` (`nextest-runner/src/record/rerun.rs`); nextest's parser fuzz target (`nextest-filtering/fuzz/fuzz_targets/fuzz_parsing.rs`).

### What a test is kept for

This section applies `general.md`'s rule to Rust. Name a test for its promise, never for whoever or whatever found it. When a type makes a refusal unreachable, remove its run-time test and keep a `compile_fail` doctest of the static promise. A case table reports the failed input. Use separately named `test-case` rows when independent reporting helps, or a loop whose assertion names the input. Assert on variants. Compare rendered text only where wording is the promise, under the snapshot rule.

Ref: ripgrep ties regression tests to issues (`tests/regression.rs`); nextest loops over identifier cases (`nextest-runner/src/config/core/identifier.rs`).

## Lints and documentation

### Lint exceptions

Put `#[expect(lint, reason = "…")]` on the narrowest item. `allow_attributes` and `allow_attributes_without_reason` enforce that form. The reason names a constraint the code cannot show. Set no size thresholds in `clippy.toml`. Keep `too_many_arguments` at Clippy's default. If required inputs exceed that limit, use an `expect` rather than a tuple or one-off struct. Do not enable `wildcard_enum_match_arm` or `use_debug`. The first flags foreign enums, and diagnostic `Debug` output is legitimate.

Before, inputs packed to satisfy a threshold:

```rust
fn ended(&mut self, met: &mut Met, (problem, outcome): (&Problem<'_>, &Solved), gripped: Gripped) -> Record
```

After:

```rust
fn ended(&mut self, met: &mut Met, problem: &Problem<'_>, outcome: &Solved, gripped: Gripped) -> Record
```

Ref: nextest `TestRunnerBuilder::build` takes its required inputs under `#[expect(clippy::too_many_arguments)]` (`nextest-runner/src/runner/imp.rs`).

### Report scans

[tools/rust_scans.py](../tools/rust_scans.py) provides three text scans for cases the lints cannot detect. The first reports file, process, environment, and clock tokens in project-named core crates ([Purity](#purity)). The second reports a primitive field or parameter whose name contains a unit, outside named kernels and tests ([Quantities](#quantities)). The third reports codes in `#[error(...)]` messages ([Messages and codes](#messages-and-codes)). These scans recognise spelling only. An aliased import escapes the first scan, and a renamed field escapes the second. Run them from the development shell as review aids. They cause no check failures.

### Documentation

This section applies `general.md`'s comment rule to Rust. Each crate's and module's `//!` describes its job, what it never knows, its design, and its reasoning at the necessary length. rust-analyzer and ripgrep put architecture in crate documentation. Document every public item's purpose and invariants. Include `# Errors` for a fallible function and `# Panics` for one that may panic. The relevant lints are `missing_docs`, `clippy::missing_errors_doc`, and `clippy::missing_panics_doc`. Include a runnable example for each public core entry point and each text format, as nextest and jiff do. Only comments inside function bodies have a length cap (`general.md`).

```rust
//! Keeping a character's limbs clear of its own body over plain values; it never reads a file, a clock or the
//! engine, and knows no bone's name.
```

Ref: ripgrep's `grep-matcher` crate doc (`crates/matcher/src/lib.rs`); jiff's doc examples, each the documented common use.

Document the current design in the owning module's `//!`, its types, and their documentation. External documents contain what the code cannot: research behind a choice, an unbuilt plan, an externally read format, and user documentation. The project's `AGENTS.md` gives a short orientation with the system's data flow and each step's entry point. It helps readers find where to start. It repeats no type, invariant, or signature, since duplicate descriptions can become stale. Do not create a separate architecture document.

## For a new project: not yet decided

A project answers these open questions in its `AGENTS.md` when it needs the relevant capability, with prior art. These are not rules yet. A project without the need, such as network access or async, forbids the capability rather than leaving it unspecified.

- **Async, concurrency, and cancellation.** Decide whether to use an async runtime. Define task and state ownership, cancellation and drop behaviour, bounded queues, timeouts, and shutdown. Address locks across awaits, blocking work off the executor, cleanup after partial failure, and tests with controlled time. Prior art includes nextest's execution acknowledgement and cancellation (`nextest-runner/src/runner/executor.rs`).
- **Network and TLS.** Decide which crates may be network clients, using a per-crate allow-list checked against the dependency graph.
- **Panics in a long-running process.** Decide how to catch and report a panic at a request or task boundary. rust-analyzer uses `catch_unwind`.
- **Tracing and redaction.** Define structured spans per operation and log levels. Address types that contain secrets or private conversations and redact them in `Debug`. Address human diagnostics on stderr and machine output on stdout.
- **Layered configuration.** Define precedence and provenance for files, environment variables, and flags. Address one-time parsing into typed state with `deny_unknown_fields`, secrets, and reload semantics. Prior art includes `nextest-runner/src/cargo_config/discovery.rs`.
- **Machine-readable command-line output.** Define semantic exit statuses, broken-pipe handling, stdout and stderr separation, and a versioned machine format. nextest-metadata provides prior art.
- **Migrations of durable data.** Data that cannot be rebuilt from sources needs schema versions, migrations, atomic and durable writes, concurrent writers, and recovery. Derived records can instead refuse an unknown version and request a rebuild. Prior art includes nextest's old-reader and new-data compatibility matrix (`AGENTS.md`).
- **Protocol stability.** A protocol with outside peers needs `#[non_exhaustive]`, versions, and compatibility tests. A published crate needs a policy for SemVer, its minimum supported Rust version, and features.
- **Wall-clock time.** Distinguish timestamps and time zones, as in jiff, from durations.
