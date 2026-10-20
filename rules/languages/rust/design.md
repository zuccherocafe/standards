# Rust standard: Design

Part of the [Rust standard](README.md). Types, parsing, where behaviour lives, visibility, records, modules and crates.

## Types and invariants

### Identities

*Weigh.* An identity in a crate-level interface, or one that is stored, has its own type where it could be confused with another identity or keys a map. A crate-level interface is a `pub(crate)` or wider signature, a record, or an error. Use a newtype with a private field, a checked constructor, an accessor, `Display`, and `Eq + Ord + Hash`. Key maps by the identity type so a key from one map does not fit another. When authored and stored names differ, the type has two entry points. One accepts the authored form. `FromStr` accepts the stored form, which must round-trip. For example, a person may write "violin case" while the program stores `violin_case`.

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

**Checked by:** review. Once an identity has its type, the compiler keeps it out of the wrong map.

### Quantities

*Default.* A quantity has a type. A unit is never merely a name. A length, angle, time, or rate in a crate-level interface uses a unit type. Examples include `Length`, `Radians`, `Seconds`, and `Rate`. Use one explicit conversion where another unit enters from an authored file. A unitless type is only for a dimensionless value. A compound dimension read by one consumer keeps its unit in its name rather than gaining a one-use type. A record keeps its wire spelling with `#[serde(rename = "margin_metres")]`.

Type crate-level interfaces and stored state, not arithmetic. A numerical kernel is a private module with one caller in its crate. Its functions may take and return primitive types and slices, unwrapped once at entry and wrapped once at exit. A unit type has only operations its callers use. Every `impl Mul<Seconds> for Rate` is a dimensional claim. The exception is temporary numerical arithmetic. A local or intermediate calculation result, and a kernel argument, remain the primitive types the arithmetic uses. Stored state and values protected by private invariants keep their types wherever they occur. [Identities](#identities) defines a crate-level interface.

Ref: jiff `SignedDuration` (`crates/jiff/src/signed_duration.rs`); nextest stores time only as `Duration` (`nextest-runner/src/config/elements/slow_timeout.rs`); jiff moved ranged integers out of its internals and kept them at the public boundary (`CHANGELOG.md`, 0.2.20).

**Checked by:** the report scans' unit scan for unit-named primitive fields and parameters; review for the rest.

### Invariants

*Always.* Establish an invariant once and preserve it on every path. An invariant type has private fields in its owning module and one checked constructor, `new(raw) -> Result<Self, ItsOwnError>`. Use `Option` instead when there is one reason to refuse. Every other construction or mutation preserves the invariant. `Deserialize` follows [Deserialising](#deserialising). `Default` exists only if the default is valid. Mutators and `From` conversions preserve the invariant. `pub(crate)` fields permit construction throughout the crate and are not private. A `const` counterpart that panics exists only where a constant needs it. It panics only during constant evaluation. Use no `new_unchecked` unless a measurement shows that checking has a cost. A function that checks a value returns the checked type, not `()` ([Make invalid states unrepresentable](../../general.md#design)).

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

**Checked by:** review; the compiler keeps private fields private once they are; Clippy's pedantic `unsafe_derive_deserialize` only for a derived `Deserialize` on a type with `unsafe` methods.

### The numerical contract

*Always.* Valid inputs do not guarantee a valid result.

- Give a combination of individually valid values that an algorithm cannot use its own named outcome. Never narrow a general type to fit one algorithm.
- Finite inputs do not guarantee finite arithmetic. Check a result that can overflow or lose definition where it is produced. Alternatively, document an input bound that proves the result finite.
- Sizes and indices use `try_from` and either `checked_*` or `saturating_*`. Document the chosen meaning. Use `as` only under `#[expect(clippy::cast_*, reason = "...")]` that names the bound.
- A tolerance is a named constant beside the type that uses it.
- Public components do not enforce an invariant. A `glam` quaternion is not unit by construction, and multiplication does not renormalise it.
- Numerical failure of accepted input is an ordinary outcome value, distinct from a refusal of an author's mistake and from a bug.
- `debug_assert!` documents an expectation. It proves nothing in a release build.

Ref: jiff checks its arithmetic before using its unchecked constructor (`crates/jiff/src/signed_duration.rs`).

**Checked by:** Clippy's pedantic `cast_*` lints for `as` casts, and `allow_attributes_without_reason` for their reasons; review for the rest.

### Closed sets

*Default.* A closed set is an enum. Match a crate's own enums exhaustively. Put case-dependent behaviour in a method on the enum. Where a new variant would need a decision, use neither an `_` arm nor `matches!` on the crate's own enum. Adding that variant then fails to compile at the relevant use.

Ref: guppy `DependencyDirection::opposite` (`guppy/src/graph/mod.rs`); nextest `ExecutionResult::as_static_str` (`nextest-runner/src/reporter/events.rs`).

**Checked by:** the compiler's exhaustive `match`, and Clippy's `match_wildcard_for_single_variants`; review for `_` arms and `matches!` on the crate's own enums.

### Sum types

*Default.* State that varies together, or may be absent, is a sum type. Replace a `bool` that selects a mode or determines other fields' meanings with an enum. Each variant contains exactly the fields for its case. Represent options set together, an `Option` tied to one value of a neighbouring enum, or parallel vectors that must agree as one variant or one vector of structs. Absence is `Option` or a named variant, never an empty string, infinity, `-1`, or zero. A `bool` that is an independent observation stays a `bool`. Before combining two flags into one enum, prove from the code that they cannot both hold. Convert a command-line or file flag to its enum at the boundary.

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

**Checked by:** Clippy's pedantic `struct_excessive_bools` and `fn_params_excessive_bools` flag many booleans together, a count rather than proof that they depend on each other; review.

### Tuples

*Default.* A domain value or interface value with its own meaning is a named type, not a bare tuple. In a crate-level interface or a stored field, same-typed values whose order carries meaning use named struct fields or a [small key set](#small-key-sets). Examples include low and high values or left and right values. A tuple would allow a caller to swap them. A numerical kernel's working pairs remain tuples under the [Quantities](#quantities) exception, such as bracket endpoints passed between its functions. Different-typed pairs that the caller unpacks at once also remain tuples. Their types already distinguish the values, so a one-use struct adds no useful name.

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

**Checked by:** review.

### Small key sets

*Default.* Represent one value per member of a small closed key set with a structure indexed by the key's enum. Never use an array with an order convention or an integer key. For example, in tdollhouse: `PerSide<T>`, indexed by `Side`, stores one value per body side. Never use `[T; 2]` with a "left first" convention or a `usize` side. With an invalid integer side, `1 - side` underflows.

```rust
pub enum Side { Left, Right }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerSide<T> { pub left: T, pub right: T }
impl<T> Index<Side> for PerSide<T> { /* match side */ }
```

`PerSide` proves that there are exactly two values and that a side is one of two enum values. It does not prove that the stored values belong to the right owner.

**Checked by:** review.

### Index spaces

*Weigh.* An index that is stored or passed between modules has its own index type, issued by its collection. Only the collection's crate can construct an index from an offset, through a sealed trait or crate-private constructor. The collection issues indices through operations such as `push` or iteration. A typed index proves its index space, not its originating instance. An index from one solver is a valid index for another. Where a handle might come from another instance, bind it to its source or check it once at entry.

Ref: guppy's private `PackageIx(u32)` (`guppy/src/graph/mod.rs`); rust-analyzer's `la-arena` `Idx<T>`.

**Checked by:** review; the compiler keeps one space's index out of another once each has its type.

### Checked against a source

*Weigh.* A value checked against a source retains that source. A value valid only with another value, such as a clip checked against one body, holds a reference to that source. Construct the checked value through its source. No operation then takes the pair separately or can combine mismatched values. The lifetime prevents the checked value from outliving its source but does not distinguish source instances. Lifetimes are not brands, so construction must go through the source. A stage gets its own type when it establishes a fact that the next operation needs. An ordinary transformation that establishes nothing new does not.

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

**Checked by:** review; the borrow checker keeps a checked value from outliving its source.

### Lifetimes

*Default.* A type carries a lifetime only when it represents a borrowed view. The borrow either binds the value to its source or avoids copying large data the caller still needs. Merely bundling borrowed arguments does not justify a struct. Pass the arguments or make the operation a method on the data's owner.

**Checked by:** review.

## Parsing at boundaries

### Parse once

*Always.* [Parse at the boundary](../../general.md#design) governs every Rust input as written. In crates that take paths, convert paths to UTF-8 once at entry, using `camino`'s `Utf8PathBuf`.

Ref: Alexis King, ["Parse, don't validate"](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/); rust-analyzer builds `CrateGraph` through constructing functions (`docs/book/src/contributing/architecture.md`).

**Checked by:** review.

### Deserialising

*Always.* Deserialisation calls the constructor. An invariant type uses `#[serde(try_from = "Raw")]` or a hand-written `Deserialize` that calls `new`. Its error has its own type, not `String`. Never use `#[serde(untagged)]`. Write a visitor with an `expecting` message. Records read back use `deny_unknown_fields`.

Ref: nextest `ConfigIdentifier`'s `Deserialize`, tested through both entry paths (`nextest-runner/src/config/core/identifier.rs`).

**Checked by:** Clippy's pedantic `unsafe_derive_deserialize` in its narrow case; review for the rest.

## Where behaviour lives

### Methods on the owner

*Default.* Behaviour that reads one type's data is a method on that type. Pure maths over plain values remains a free function. A free function implements an algorithm over peer inputs with no owner, such as distance between shapes or a spline, or is a private helper.

**Checked by:** review.

### Standard traits

*Default.* Use the standard trait that fits the operation. Use `Display` for human-readable values and `FromStr` for stored text syntax. Use `From` and `TryFrom` for boundary conversions, and `Index` for keyed access. Use `IntoIterator` and `Iterator` for traversal. Use `Deref` only for a collection newtype.

Ref: nextest `TestThreads` with `FromStr`, `Display`, and a visitor (`nextest-runner/src/config/elements/test_threads.rs`).

**Checked by:** Clippy's `should_implement_trait`, `inherent_to_string`, `inherent_to_string_shadow_display`, `from_over_into`, and `to_string_trait_impl`, and the pedantic `iter_without_into_iter` and `iter_not_returning_iterator`, each for its own case; review.

### Traits

*Default.* Do not introduce a trait merely for one implementation. A trait exists where several types implement it or where it inverts a dependency a core must not have. In the latter case, the core declares the trait and hosts implement it without adding the host dependency to the core. An adaptor trait for foreign types at a named boundary is allowed. A test double alone does not justify a trait, as [Code is not shaped for its tests](../../general.md#checks) says. When a test needs made-up inputs, put the algorithm in a plain function over values that both the test and the production type's inherent method call. Production behaviour remains an inherent method.

Ref: nextest `StoreReader` with two implementations (`nextest-runner/src/record/reader.rs`); guppy's `IntoMietteDiagnostic` adapting foreign errors (`target-spec-miette/src/imp.rs`).

**Checked by:** review.

## Visibility

*Default.* The public API is what other crates use. An item is `pub` only when another crate names it. A module is `pub` only to group such items. Everything else is `pub(crate)` or narrower, enforced by rustc's `unreachable_pub`. A field is public only when every value of its type is valid there. A struct has either all public fields or none, enforced by Clippy's `partial_pub_fields`. A crate exports types and operations, not internal tuning. Caller-variable settings are fields of a settings type with a `const DEFAULT`. Tuning that another crate must read is exported read-only and accepted by no function. A sealed trait may hide a constructor. `#[non_exhaustive]` follows a compatibility promise. A workspace whose crates are not published makes no such promise. The public API report lists each library crate's public items before and after a change. It is dandegate's `api` step ([tools/crates/dandegate/](../../../tools/crates/dandegate/)), which `--with api` selects. Reviewers read these promises before the implementation. The report causes no check failures.

Ref: nextest's `AGENTS.md` ("Use `pub(crate)` and `pub(super)` liberally"); nextest's raw serde shapes are `pub(in crate::config)` (`nextest-runner/src/config/overrides/imp.rs`).

**Checked by:** rustc's `unreachable_pub` and Clippy's `partial_pub_fields`; the public API report and review for what is public and why.

## Records and serialisation

### One type per representation

*Default.* Under [One home per concept](../../general.md#design), a concept has one type for each genuinely different representation, such as a persistent name or an index bound to a live collection. An opaque name needed by several crates lives in their lowest shared crate. An index lives with its issuing collection. A type used by one crate lives there.

Ref: nextest-filtering imports `RustBinaryId` from nextest-metadata (`nextest-filtering/src/expression.rs`); rust-analyzer chooses boundary counterparts deliberately (`docs/book/src/contributing/architecture.md`).

**Checked by:** review.

### Records

*Default.* A record serialises the core's report type or one wire type with one conversion. Core report types derive `Serialize` and `Deserialize` behind an optional `serde` feature enabled by the writing crate. Working inputs never do. A separate wire type exists only when the wire form differs, such as names instead of indices, a projection, or a legacy shape. It has exactly one conversion and a round-trip test against a hand-written fixture file. A change to a written file's bytes states the change and names the file and format tag. It updates every reader in the same commit. Disk collections are ordered, using `BTreeMap` or `IndexMap`. `HashMap` and `HashSet` serve lookups only.

Ref: guppy serialises `guppy-summaries`, not `PackageGraph`; nextest serialises `nextest-metadata` summaries, not its test list; ripgrep's `grep-printer` keeps serde behind a feature (`crates/printer/Cargo.toml`).

**Checked by:** a wire type's round-trip test against its hand-written fixture; review.

### Debug

*Always.* `Debug` is for diagnostics. It is never a stable encoding, record field, identity, or cache-key input. A record names a case through serde, `Display`, or an `as_str` with one arm per variant. Diagnostic `Debug` output in logs and test failures is allowed.

Before, renaming a variant silently invalidates every cache:

```rust
.map(|(role, named)| (format!("{role:?}"), named.bone.as_str()))
```

After: the role's serde spelling, or an `as_str` with one arm per variant.

**Checked by:** review.

## Modules, crates, and purity

### Modules

*Default.* Keep one cohesive concept per module. Cooperating types may share a file, and a large concept may occupy one large file. A parent may contain a façade's entry functions, declarations, and re-exports, so splitting a file never changes a caller's path. Merge files too small to stand alone into their owner. Group flat directories by subject. A module directory is `name/` beside `name.rs`, never `mod.rs`, as `clippy::mod_module_files` enforces. Tests sit beside their subject in `#[cfg(test)] mod tests`. They may use a named test module when shared fixtures make that more cohesive. Scenario tests live in the crate's `tests/`. Impose no file-count or one-type-per-file rules.

Ref: nextest `config/elements/`, one element per file with its impls and tests; guppy's `graph_impl.rs` holds several cooperating types.

**Checked by:** Clippy's `mod_module_files`; review.

### Crates

*Default.* A crate exists for a reason the compiler or build can check. Such reasons include a different licence or artefact, a confined heavy or platform dependency, an absence checked by the crate graph, or independent use by another crate. An artefact may be a host library, a binary, or a binary that nothing may link. Size, a pipeline stage, or a group of helpers does not justify a crate. Use modules for those. A crate directory takes its crate's name. Each package's `Cargo.toml` has a one-sentence `description` of the crate's purpose, agreeing with its first `//!` line.

Ref: matklad, ["Fast Rust Builds"](https://matklad.github.io/2021/09/04/fast-rust-builds.html); ripgrep's `grep-matcher` depends only on `memchr`.

**Checked by:** the project's dependency-edge check where it has one; review.

### Purity

*Always.* Cores compute. The shell does input and output. An engine-independent core reads no file, clock, environment variable, or process and spawns nothing. Purity applies at crate level, and the dependency list is the cheapest proof. Purity alone does not justify a crate because `std::fs` is in every crate's graph. Inside an input-output crate, purity applies at function level. Each stage's logic is a named function over values, tested on values. Named modules perform input and output at the boundaries. Document a clock read that only reports timings and changes no output in the crate's documentation.

Ref: rust-analyzer's core "never does any IO" (`docs/book/src/contributing/architecture.md`); nextest-runner does IO throughout and keeps pure functions such as `compute_outstanding_pure` (`nextest-runner/src/record/rerun.rs`).

**Checked by:** the dependency list, the project's dependency-edge check where it has one, and the report scans' purity scan; review.
