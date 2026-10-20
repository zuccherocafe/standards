# Rust standard: Tests

Part of the [Rust standard](README.md). The test policy, scenarios, expected values, properties and the mutation report.

## Tests

### The test policy

*Default.* A crate's tests come in the three layers of [Test the common scenario first](../../general.md#checks), written in that order, and a review reads them in the same order. Scenarios live in `tests/scenarios.rs` ([Scenarios first](#scenarios-first)), each function's unit tests sit beside that function, and properties follow [Properties](#properties).

Expected values follow [Expected values and snapshots](#expected-values-and-snapshots), and a test is kept as [What a test is kept for](#what-a-test-is-kept-for) says. A scenario's smallest input may be a code-built character or a fixture with a few files. A minutes-long run over real data belongs in the change report, not the checks. The report shows what changed.

**Checked by:** review; the mutation report lists mutations the tests miss, and review decides whether each breaks a promise.

### Scenarios first

*Default.* Each crate's `tests/scenarios.rs` begins with module documentation listing the crate's uses, most common first, and holds each use's test ([Test the common scenario first](../../general.md#checks)). A scenario binary split across files keeps `tests/scenarios.rs` as its root, with its parts in `tests/scenarios/`. Examples of a scenario include a command-line build, an import, a played clip, an unchanged-input rebuild that writes nothing new, or a solve on made-up input through the public entry point. A small constructor or kernel needs no committed fixture.

For example, in tdollhouse: the first scenario covers a spring chain trailing a moving parent and settling back, its most common behaviour. Choose its threshold under [Checks fail only on broken results](../../general.md#checks).

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

**Checked by:** review.

### Expected values and snapshots

*Always.* Expected values follow [Expected values are authored](../../general.md#checks). Snapshots use `insta`, and a reviewer accepts each one in the diff with `cargo insta review`.

```rust
// Authored from the fixture's own files: it holds these four animations.
assert_eq!(manifest.clip_names(), BTreeSet::from(["Nod", "Rest", "Stride", "Turn"]));

// Exact text whose wording is the promise.
insta::assert_snapshot!(render(&check(&fixture().with(r#"model "model.obj""#))));
```

Ref: nextest's integration snapshots of help, errors, and diagnostics (`integration-tests/tests/integration/snapshots/`); nextest's hand-written fixture model (`fixture-data/src/fixture_project.rs`).

**Checked by:** insta's snapshot assertions compare output with the saved snapshot; review for whether each expected value and snapshot is authored, right, and used only where its shape is the promise.

### Properties

*Weigh.* Properties test what types cannot express. Generate valid-domain values through domain constructors. Generate malformed input, such as arbitrary text or numbers, at a parsing boundary to test refusal. A loud failure that only generated malformed input reaches, in offline code whose boundary reads no outside party's input, is a should-fix finding. A silently wrong result, a broken stated promise, or a panic on an engine path is must-fix, whatever input reaches it. Prefer an oracle, then a round trip, then an invariance promised by the specification. Examples include stable output for the same input, independence from unit order, or preservation of already-clear values. Where a numerical or behavioural promise has a cheap oracle, a property may cover it as well as its hand-written expectations.

*Always.* Never test a property the code does not promise. Property tests in the checks use a fixed seed and case budget so rerunning the test reproduces a failure. They write nothing. Turn off proptest's default persistence in every configuration because it writes a regressions file beside the test. Explore fresh seeds by hand. Keep a counterexample to a real promise as an ordinary unit test named for that promise.

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

**Checked by:** review.

### What a test is kept for

*Default.* Tests are kept and named as [Keep a test for a promise](../../general.md#checks) says. Answer a review finding whose input nothing real produces with the type or reasoning, not a test. When a type makes a refusal unreachable, the test of the static promise is a `compile_fail` doctest. A case table reports the failed input. Use separately named `test-case` rows when independent reporting helps, or a loop whose assertion names the input. Assert on variants. Compare rendered text only where wording is the promise, under the snapshot rule.

Ref: ripgrep ties regression tests to issues (`tests/regression.rs`); nextest loops over identifier cases (`nextest-runner/src/config/core/identifier.rs`).

**Checked by:** rustdoc for `compile_fail` doctests; review.

### Mutation report

*Default.* `rust-mutants`, built from [tools/crates/rust-mutants/](../../../tools/crates/rust-mutants/) and exported by the standards' flake, runs [cargo-mutants](https://mutants.rs/) over the Rust a change touched. It makes one edit at a time, such as `>` to `>=`, `-` to `+`, or a function returning a default, and runs the crate's tests against each. It lists each edit the tests miss. A missed edit is a candidate, not proof of a weak test: judgement decides whether it breaks a promise. A change to Rust runs the report against its base, and keeps it with its evidence, when its brief picks it. dandegate's built-in `mutants` step runs the report on the Rust that changed between a change report's two revisions, with the report's own `mutants` profile. The writer judges each missed mutant as one of three kinds. A real gap is a wrong result no test notices, and the change adds the test that does. An equivalent mutant changes no behaviour, and the writer says why in one line. An unstated edge is a boundary the code never promised, such as which side an exact tie falls on. Where a reader depends on it, the writer states the rule in the code's documentation, then tests it as a real gap. Where none does, the writer says so in one line. The maintainer decides a rule the writer cannot. A mutant that times out is listed separately: it usually makes a loop run forever, which the tests already notice by stopping, so the writer checks the time allowance and adds no test for it. A mutant that allocates without end is killed by the memory cap the report runs its shards under, and the run goes on. Each test binary runs in a PID namespace of its own, so a mutant in code that signals processes reaches only those its test started. Never write a test only to catch a mutant, without a promise behind it. The report causes no check failures, since judging a miss needs a reviewer and its cost grows with the change. A run over a whole crate is a manual report, useful before trusting a crate's tests. The project pins cargo-mutants in its development shell.

**Checked by:** review of the report and each judgement.
