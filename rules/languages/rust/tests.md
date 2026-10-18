# Rust standard: Tests

Part of the [Rust standard](README.md). The test policy, scenarios, expected values, properties and the mutation report.

## Tests

### The test policy

A crate's tests come in three layers, written in this order, and a review reads them in the same order.

1. **Scenarios.** Write one test per use through the public entry point where the crate is used, most common use first. Examples include a command-line build, an import, a played clip, or an unchanged-input rebuild that writes nothing new.
2. **Function promises.** Write unit tests of each function's promises beside that function, using small made-up inputs.
3. **Properties.** Test invariants that types cannot express, using values generated from the valid domain or malformed input at a boundary.

Write expected values from the input's source, a specification, or a law, never from a run. A reviewer-accepted snapshot of exact text or structured output whose shape is the promise also counts as authored ([Expected values and snapshots](#expected-values-and-snapshots)). Keep a test that replays a review finding only when real data produces its input. Name it for its promise. For an input nothing real produces, answer the finding with the type or reasoning rather than a test. A scenario in the checks costs seconds. Use the smallest input that reaches the path, such as a code-built character or a fixture with a few files. A minutes-long run over real data belongs in the change report, not the checks. The report shows what changed.

**Checked by:** review; the mutation report lists mutations the tests miss, and review decides whether each breaks a promise.

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

**Checked by:** review.

### Expected values and snapshots

Expected numbers and behaviour are written by hand from the fixture's source, a specification, or a law, never copied from a run. Snapshots with `insta` cover exact text or structured output whose shape is the promise, such as help text, a rendered error, or a listing. A reviewer-accepted snapshot in the diff (`cargo insta review`) is an authored expectation. Never snapshot numbers or behaviour. Keep hand-written expectations and properties for them. The project's change report shows differences.

```rust
// Authored from the fixture's own files: it holds these four animations.
assert_eq!(manifest.clip_names(), BTreeSet::from(["Nod", "Rest", "Stride", "Turn"]));

// Exact text whose wording is the promise.
insta::assert_snapshot!(render(&check(&fixture().with(r#"model "model.obj""#))));
```

Ref: nextest's integration snapshots of help, errors, and diagnostics (`integration-tests/tests/integration/snapshots/`); nextest's hand-written fixture model (`fixture-data/src/fixture_project.rs`).

**Checked by:** insta's snapshot assertions compare output with the saved snapshot; review for whether each expected value and snapshot is authored, right, and used only where its shape is the promise.

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

**Checked by:** review.

### What a test is kept for

This section applies `general.md`'s rule to Rust. Name a test for its promise, never for whoever or whatever found it. When a type makes a refusal unreachable, remove its run-time test and keep a `compile_fail` doctest of the static promise. A case table reports the failed input. Use separately named `test-case` rows when independent reporting helps, or a loop whose assertion names the input. Assert on variants. Compare rendered text only where wording is the promise, under the snapshot rule.

Ref: ripgrep ties regression tests to issues (`tests/regression.rs`); nextest loops over identifier cases (`nextest-runner/src/config/core/identifier.rs`).

**Checked by:** rustdoc for `compile_fail` doctests; review.

### Mutation report

[tools/rust_mutants.py](../../../tools/rust_mutants.py) runs [cargo-mutants](https://mutants.rs/) over the Rust a change touched. It makes one edit at a time, such as `>` to `>=`, `-` to `+`, or a function returning a default, and runs the crate's tests against each. It lists each edit the tests miss. A missed edit is a candidate, not proof of a weak test: judgement decides whether it breaks a promise. Every change to Rust runs the report against the change's base and keeps it with its evidence. The writer judges each missed mutant as one of three kinds. A real gap is a wrong result no test notices, and the change adds the test that does. An equivalent mutant changes no behaviour, and the writer says why in one line. An unstated edge is a boundary the code never promised, such as which side an exact tie falls on. The writer states the rule in the code's documentation, then tests it as a real gap. The maintainer decides a rule the writer cannot. A mutant that times out is listed separately: it usually makes a loop run forever, which the tests already notice by stopping, so the writer checks the time allowance and adds no test for it. A mutant that allocates without end is killed by the memory cap the report runs its shards under, and the run goes on. Each test binary runs in a PID namespace of its own, so a mutant in code that signals processes reaches only those its test started. Never write a test only to catch a mutant, without a promise behind it. The report causes no check failures, since judging a miss needs a reviewer and its cost grows with the change. A run over a whole crate is a manual report, useful before trusting a crate's tests. The project pins cargo-mutants in its development shell.

**Checked by:** review of the report and each judgement.
