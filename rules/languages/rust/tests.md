# Rust standard: Tests

Part of the [Rust standard](README.md). This file covers the test policy, scenarios, expected values, properties, and the mutation report.

## Tests

### The test policy

*Default.* Write and review a crate's tests in the three layers of [Test the common scenario first](../../general.md#checks), in that order: scenarios in `tests/scenarios.rs` ([Scenarios first](#scenarios-first)), unit tests beside each function, and property tests ([Properties](#properties)). Expected values follow [Expected values and snapshots](#expected-values-and-snapshots), and a test is kept as [What a test is kept for](#what-a-test-is-kept-for) says. Keep each scenario's input small, such as a value built in code or a fixture with a few files, and put any minutes-long run over real data in the change report rather than in the checks so the report shows what changed.

**Checked by:** review; the mutation report lists mutations the tests miss, and review decides whether each breaks a promise.

### Scenarios first

*Default.* Each crate's `tests/scenarios.rs` begins with module documentation listing the crate's uses, most common first, and holds each use's test ([Test the common scenario first](../../general.md#checks)). A scenario binary split across files keeps `tests/scenarios.rs` as its root, with its parts in `tests/scenarios/`. Examples of a scenario include a command-line build, an import, a played clip, an unchanged-input rebuild that writes nothing new, or a solve on made-up input through the public entry point. A small constructor or kernel needs no committed fixture.

For example, in tdollhouse: the first scenario covers a spring chain trailing a moving parent and settling back, its most common behaviour. Choose its threshold under [Checks fail only on broken results](../../general.md#checks).

```rust
/// Hair behind a turning head trails it, swings, and settles where the clip points it.
#[test]
fn a_chain_trails_a_turning_parent_then_settles_where_the_clip_points_it() {
    // ...
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

*Weigh.* Use property tests for invariants that types cannot express. Generate valid domain values through their constructors, and generate malformed inputs, such as arbitrary text or numbers, at parsing boundaries to test refusals. In offline code whose boundary reads no outside party's input, a loud failure that only generated malformed input can reach is a should-fix finding; a silently wrong result, a broken stated promise, or a panic on an engine path is must-fix whatever input reaches it. Prefer an oracle first, then a round trip, and then an invariance promised by the specification, such as identical output for identical input, independence from unit order, or preserving already-clear values. When a numerical or behavioural promise has a cheap oracle, a property test may cover it alongside its hand-written unit tests.

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

*Default.* Keep and name tests as [Keep a test for a promise](../../general.md#checks) specifies. When a review finding rests on an input that nothing real produces, answer it with a type or an explanation rather than adding a test. When a type makes a refusal unreachable at run time, test the static promise with a `compile_fail` doctest. In table-driven tests, make a failure name the offending input, either with separately named `test-case` rows when independent reporting helps or with a loop whose assertion prints the input. Assert on variants, and compare rendered text only when the exact wording is the promise ([Expected values and snapshots](#expected-values-and-snapshots)).

Ref: ripgrep ties regression tests to issues (`tests/regression.rs`); nextest loops over identifier cases (`nextest-runner/src/config/core/identifier.rs`).

**Checked by:** rustdoc for `compile_fail` doctests; review.

### Mutation report

*Default.* A change to Rust runs the mutation report against its base, and keeps it with its evidence, when its brief picks it. dandegate's built-in `mutants` step runs `rust-mutants` ([tools/crates/rust-mutants/](../../../tools/crates/rust-mutants/), exported by the standards' flake with the [cargo-mutants](https://mutants.rs/) it pins) on the Rust that changed between a change report's two revisions, in every change report that includes it. A later report reuses its result only while what it reads stays the same ([Generated output is typed and trusted](../../general.md#design)), as [dandegate's Rust reports](../../../tools/crates/dandegate/src/rust.rs) describe. You can also run `rust-mutants` manually across a whole crate before trusting its test suite. The tool makes one small edit at a time, such as changing `>` to `>=`, `-` to `+`, or a function body to a default return value, runs the crate's tests under the report's `mutants` profile, and lists every edit the tests miss. The report runs its shards under a memory cap, so a mutant that allocates without bound is killed and the run goes on. Each test binary runs in its own PID namespace, so a mutant in code that signals processes can reach only children that its own test started. Because judging a miss needs a reviewer and the run time grows with the change, the report causes no check failures.

A missed mutant is a candidate for review, not automatic proof of a weak test; never write a test only to catch a mutant when no promise is behind it. For each missed mutant, the writer records one of three judgements:

- **Real gap.** The edit produces a wrong result that no test notices, and the change adds the test that does.
- **Equivalent mutant.** The edit changes no observable behaviour, and the writer explains why in one line.
- **Unstated edge.** The edit changes behaviour at a boundary the code never promised, such as which side an exact tie falls on. If a reader depends on that boundary, the writer states the rule in the code's documentation and tests it as a real gap; if none does, the writer says so in one line. The maintainer decides any rule the writer cannot.

Timed-out mutants appear in their own list because they usually make a loop run forever, which the tests already notice by stopping, so the writer checks the time allowance and adds no test for it.

**Checked by:** review of the report and each judgement.
