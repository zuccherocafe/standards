# Rust standard: Design

Part of the [Rust standard](README.md). This file covers types, parsing, where behaviour lives, visibility, records, modules, and crates.

## Types and invariants

### Identities

*Weigh.* Give an identity its own type when it appears in a crate-level interface or in stored state, and when it keys a map or could be confused with another identity. A crate-level interface is a `pub(crate)` or wider signature, a record, or an error. Use a newtype with a private field, a checked constructor, an accessor, `Display`, and `Eq + Ord + Hash`. Key each map by its identity type so a key from one map cannot index another. When authored and stored names differ, give the type two entry points: one for the authored form, such as `"violin case"`, and `FromStr` for the stored form, such as `violin_case`, which must round-trip.

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

*Default.* Give every quantity in a crate-level interface or stored state (such as a length, angle, time, or rate) its own unit type, such as `Length`, `Radians`, `Seconds`, or `Rate`, rather than putting the unit only in the name. Convert once where another unit enters from an authored file, and keep the wire field name with `#[serde(rename = "margin_metres")]`. Use a unitless type only for a dimensionless value. When a compound dimension has only one consumer, keep the unit in the field or parameter name instead of adding a one-use type.

Type crate-level interfaces ([Identities](#identities)) and stored state, not temporary numerical arithmetic. Local intermediate values and arguments to a numerical kernel, which is a private module with one caller in its crate, may use primitive types and slices, unwrapped once on entry and wrapped once on exit. Stored state and values protected by a private invariant keep their types everywhere. Give a unit type only the operations its callers use, because an implementation such as `impl Mul<Seconds> for Rate` states a dimensional relationship.

Ref: jiff `SignedDuration` (`crates/jiff/src/signed_duration.rs`); nextest stores time only as `Duration` (`nextest-runner/src/config/elements/slow_timeout.rs`); jiff moved ranged integers out of its internals and kept them at the public boundary (`CHANGELOG.md`, 0.2.20).

**Checked by:** the report scans' unit scan for unit-named primitive fields and parameters; review for the rest.

### Invariants

*Always.* Establish an invariant once and preserve it on every path. An invariant type has private fields in its owning module (`pub(crate)` fields allow construction across the crate and are not private) and one checked constructor, `new(raw) -> Result<Self, ItsOwnError>`. Use `Option` instead when there is one reason to refuse. A function that checks a value returns the checked type, not `()` ([Make invalid states unrepresentable](../../general.md#design)). Every other construction or mutation must also preserve the invariant: `Deserialize` follows [Deserialising](#deserialising), `Default` exists only when the default value is valid, and mutators and `From` conversions keep the value valid. Add a panicking `const` constructor only when a constant needs one, and make it panic only during constant evaluation. Add `new_unchecked` only when a measurement shows that the check has a cost.

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

- When an algorithm cannot accept a combination of individually valid inputs, return a named outcome for that case rather than narrowing a general type to fit one algorithm.
- Finite inputs do not guarantee finite arithmetic. Check any result that can overflow or produce `NaN` or infinity where it is computed, or document an input bound that proves the result is finite.
- Convert sizes and indices with `try_from`, and use `checked_*` or `saturating_*` arithmetic, documenting which one applies. Use `as` only with `#[expect(clippy::cast_*, reason = "...")]` naming the bound that makes the cast safe.
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

*Default.* Use a sum type for state that varies together or may be absent. Replace a `bool` that selects a mode or changes what other fields mean with an enum whose variants hold only the fields for their case. Fold options that are always set together, or an `Option` that applies only to one variant of a neighbouring enum, into that variant. Replace parallel vectors that must agree with one vector of structs. Represent absence as `Option` or a named variant, never as an empty string, infinity, `-1`, or zero. A `bool` that is an independent observation stays a `bool`. Before combining two flags into one enum, prove from the code that they cannot both hold. Convert a command-line or file flag to its enum at the boundary.

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

*Default.* Give a domain or interface value with its own meaning a named type rather than a bare tuple. When two values in a crate-level interface or a stored field share a type and their order carries meaning, such as low and high bounds or left and right values, use named struct fields or a [small key set](#small-key-sets) so a caller cannot swap them. Keep tuples in two places: working pairs inside a numerical kernel under the [Quantities](#quantities) exception, such as bracket endpoints passed between its functions, and pairs of different types that the caller unpacks immediately, where the types already distinguish the values.

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

*Weigh.* When a value is valid only with the source it was checked against, such as a clip checked against one body, store a reference to that source inside the checked value and construct it through a method on the source. No operation then takes the pair separately or can combine mismatched values. Because a lifetime only stops the checked value from outliving its source and cannot tell two instances of the source type apart, only constructing the value through its source ties it to that instance. Give a pipeline stage its own type when it proves a fact that the next operation relies on, not for an ordinary transformation that proves nothing new.

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

*Always.* Follow [Parse at the boundary](../../general.md#design). In crates that take paths, convert each path to `camino::Utf8PathBuf` once at entry.

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

*Default.* Define a trait only when several types implement it, when it inverts a dependency that a core crate must not carry (so the core declares the trait and host crates implement it), or when it adapts a foreign type at a named boundary. Do not introduce a trait merely for one implementation or only for a test double ([Do not shape production code for its tests](../../general.md#checks)). When a test needs made-up inputs, keep the production entry point as an inherent method and put the algorithm in a plain function over values that both the method and the test call.

Ref: nextest `StoreReader` with two implementations (`nextest-runner/src/record/reader.rs`); guppy's `IntoMietteDiagnostic` adapting foreign errors (`target-spec-miette/src/imp.rs`).

**Checked by:** review.

## Visibility

*Default.* Make an item `pub` only when another crate names it, and make a module `pub` only to group such items. Keep everything else `pub(crate)` or narrower, as rustc's `unreachable_pub` enforces. Make a struct's fields public only when every value of their types is valid, and make either all of its fields public or none, as Clippy's `partial_pub_fields` enforces. Export types and operations rather than internal tuning: put settings a caller may vary on a settings struct with a `const DEFAULT`, and export any tuning constant that another crate must read as read-only, accepted by no function. A sealed trait may hide a constructor. Add `#[non_exhaustive]` only when a crate promises compatibility across versions, which an unpublished workspace crate does not. The public API report lists each library crate's public items before and after a change; reviewers read these promises before the implementation, and the report causes no check failures. The public API report is dandegate's `api` step, which `--with api` selects.

Ref: nextest's `AGENTS.md` ("Use `pub(crate)` and `pub(super)` liberally"); nextest's raw serde shapes are `pub(in crate::config)` (`nextest-runner/src/config/overrides/imp.rs`).

**Checked by:** rustc's `unreachable_pub` and Clippy's `partial_pub_fields`; the public API report and review for what is public and why.

## Records and serialisation

### One type per representation

*Default.* Under [One home per concept](../../general.md#design), a concept has one type for each genuinely different representation, such as a persistent name or an index bound to a live collection. An opaque name needed by several crates lives in their lowest shared crate. An index lives with its issuing collection. A type used by one crate lives there.

Ref: nextest-filtering imports `RustBinaryId` from nextest-metadata (`nextest-filtering/src/expression.rs`); rust-analyzer chooses boundary counterparts deliberately (`docs/book/src/contributing/architecture.md`).

**Checked by:** review.

### Records

*Default.* Serialise either the core's report type directly or one wire type with one conversion. Core report types derive `Serialize` and `Deserialize` behind an optional `serde` feature that the writing crate enables; working input types never derive serde traits. Add a separate wire type only when the wire form differs from the core type, as when it stores names instead of indices, a projection, or a legacy shape. Give that wire type exactly one conversion and a round-trip test against a hand-written fixture file. When a change alters a written file's bytes, state the change and name the file and format tag, and update every reader in the same commit. Use ordered collections (`BTreeMap` or `IndexMap`) for data written to disk, and keep `HashMap` and `HashSet` for lookups only.

Ref: guppy serialises `guppy-summaries`, not `PackageGraph`; nextest serialises `nextest-metadata` summaries, not its test list; ripgrep's `grep-printer` keeps serde behind a feature (`crates/printer/Cargo.toml`).

**Checked by:** a wire type's round-trip test against its hand-written fixture; review.

### Debug

*Always.* Use `Debug` only for diagnostics, such as logs and test failures, never as a stable encoding, record field, identity, or cache-key input. In a record, format an enum variant through serde, `Display`, or an `as_str` method with one arm per variant.

Before, renaming a variant silently invalidates every cache:

```rust
.map(|(role, named)| (format!("{role:?}"), named.bone.as_str()))
```

After: the role's serde spelling, or an `as_str` with one arm per variant.

**Checked by:** review.

## Modules, crates, and purity

### Modules

*Default.* Keep one cohesive concept per module. Cooperating types may share a file, and a large concept may stay in one large file; impose no file-count or one-type-per-file limits. Put a parent module's entry functions, submodule declarations, and re-exports in `name.rs` with submodules in `name/`, never `mod.rs` (`clippy::mod_module_files`), so splitting a file never changes a caller's path. Merge files that are too small to stand alone into their owner, and group flat directories by subject. Keep unit tests beside their subject in `#[cfg(test)] mod tests`, or in a named test module when shared fixtures make that more cohesive, and put scenario tests in the crate's `tests/` ([Scenarios first](tests.md#scenarios-first)).

Ref: nextest `config/elements/`, one element per file with its impls and tests; guppy's `graph_impl.rs` holds several cooperating types.

**Checked by:** Clippy's `mod_module_files`; review.

### Crates

*Default.* A crate exists for a reason the compiler or build can check. Such reasons include a different licence or artefact, a confined heavy or platform dependency, an absence checked by the crate graph, or independent use by another crate. An artefact may be a host library, a binary, or a binary that nothing may link. Size, a pipeline stage, or a group of helpers does not justify a crate. Use modules for those. A crate directory takes its crate's name. Each package's `Cargo.toml` has a one-sentence `description` of the crate's purpose, agreeing with its first `//!` line.

Ref: matklad, ["Fast Rust Builds"](https://matklad.github.io/2021/09/04/fast-rust-builds.html); ripgrep's `grep-matcher` depends only on `memchr`.

**Checked by:** the project's dependency-edge check where it has one; review.

### Purity

*Always.* Cores compute, and shell crates do input and output. An engine-independent core crate reads no file, clock, environment variable, or process, and spawns nothing. At the crate level, the dependency list is the cheapest proof of purity, though purity alone does not justify a new crate because `std::fs` is available in every crate. Inside an input-output crate, purity applies at the function level: keep each stage's logic in a named function over values, tested on values, and keep input and output in named boundary modules. If a crate reads the clock only to report timings without changing any output, document that in the crate's top-level documentation.

Ref: rust-analyzer's core "never does any IO" (`docs/book/src/contributing/architecture.md`); nextest-runner does IO throughout and keeps pure functions such as `compute_outstanding_pure` (`nextest-runner/src/record/rerun.rs`).

**Checked by:** the dependency list, the project's dependency-edge check where it has one, and the report scans' purity scan; review.
