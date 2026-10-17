# Review rubric

Review every change to a zucchero✿café project with these questions, whoever wrote it. Tools cover the lints and project checks. These questions cover what the tools cannot answer. Answer each with **pass**, **not applicable**, or a **finding**. Each finding cites its supporting rule in [general.md](general.md) or the language standard, [rust.md](languages/rust.md) or [gdscript.md](languages/gdscript.md). Fix the finding in the change that introduced it. Never add a run-time check where a type would suffice.

## First

Five questions come before the rest, and a change that fails one is not ready, however well it answers the others.

1. **Invalid states.** Can the new types hold a value the domain forbids?
2. **One home.** Does each concept the change touches have exactly one home?
3. **The common path.** Does a test reach the changed code through its most common real use?
4. **Correct.** Is the change correct for every correct input, not only tested inputs?
5. **Simpler.** Is there a simpler shape that is still correct?

The sections below break each into the questions a reviewer answers line by line.

## General

1. **Invalid states.** Can a new field, parameter, or return value hold a forbidden domain value? Can individually valid inputs produce an inapplicable or non-finite result without a named outcome? ([Make invalid states unrepresentable](general.md#design))
2. **Boundaries.** Does anything re-check a value past its parsing boundary? If a check was removed, does a type make its failure impossible, or does evidence show that real data cannot reach it? Does every refusal of an author's mistake name the file, line, and field? ([Parse at the boundary](general.md#design))
3. **One home.** Does the concept already have a home? Is a new representation justified by a genuinely different form, with one explicit conversion? ([One home per concept](general.md#design))
4. **Knowledge as data.** Does general code now contain input-specific knowledge that belongs in the input's files, such as a name, mapping, or case? ([Knowledge is data](general.md#design))
5. **Who fixes each error.** Is each new failure the author's, the environment's, a reader program's, or ours? Is a bug carried as a refusal, or an author's mistake treated as a bug?
6. **Written files.** Does every written file remain byte-equal? If not, does the change identify the difference and format tag? Does it update every reader in the same commit?
7. **Scenarios and expectations.** Which scenario does the change touch? Does a test reach the changed code? Was every expected value hand-written or, for exact text or structured output whose shape is the promise, accepted as a reviewed snapshot? Is any test kept for an input nothing real produces? Does any scenario in the checks take more than seconds? Does any property test use a fresh seed or write a file? ([Checks](general.md#checks), [The test policy](languages/rust.md#the-test-policy))
8. **Checks.** Does the change add or widen a check above a unit test? If so, does it state the failure caught, why unit tests cannot catch it, and the added running time? Has the maintainer approved it? Does it fail on a difference rather than a broken result? ([Checks](general.md#checks))
9. **Simplicity.** Is there validation, a refusal, a type, or a test that nothing real needs? Is anything simpler at the cost of being wrong for some correct input? ([Simplify, never at the cost of correctness](general.md#design))
10. **Comments and documentation.** Does a comment state what a type could enforce? Does any text narrate history or describe a former state? Does each module explain its design in its own documentation? Is every public item documented, rather than explained only in a distant specification? ([Writing](general.md#writing))
11. **Performance.** Does the change touch a hot path? If so, does it report before-and-after cost using a measure unaffected by unrelated machine activity? Is any budget exceeded? Does a claim of faster or free state what was measured? ([Performance](general.md#performance))
12. **Toolchain.** Is a new tool or changed version added to the development shell in the same change as its version check? Is it pinned by exact version and hash? Is a GPU program the publisher's unchanged binary? ([Toolchain](general.md#toolchain))

## Rust

1. **Wrong owner.** Can an identity, index, or checked value be used with the wrong collection, instance, or source? ([Identities](languages/rust.md#identities), [Index spaces](languages/rust.md#index-spaces), [Checked against a source](languages/rust.md#checked-against-a-source))
2. **Invariants on every path.** Does every construction or mutation preserve an invariant type's invariant? Check constructors, `Deserialize`, `Default`, mutators, conversions, and `pub(crate)` fields. ([Invariants](languages/rust.md#invariants), [Deserialising](languages/rust.md#deserialising))
3. **Numbers.** Is a quantity a bare `f64` in a crate-level interface ([Identities](languages/rust.md#identities))? The report scans identify quantities with unit-bearing names. Can finite inputs overflow or lose definition without a check? Does an `as` cast lack its bound? ([Quantities](languages/rust.md#quantities), [The numerical contract](languages/rust.md#the-numerical-contract))
4. **Shapes of state.** Does a `bool` mode, sentinel, enum-related `Option`, or set of parallel vectors need a sum type? Does a new project-enum variant fail to compile where it needs a decision? Outside a numerical kernel's working pairs, does a same-typed tuple in a domain or interface value carry meaning through order? Are bare primitive types used beyond temporary numerical arithmetic, in stored state, behind private invariants, or in crate-level interfaces? ([Sum types](languages/rust.md#sum-types), [Closed sets](languages/rust.md#closed-sets), [Tuples](languages/rust.md#tuples), [Quantities](languages/rust.md#quantities))
5. **Public API.** Read the public API report first. Is anything public that no other crate uses? Is any field public whose type admits invalid values? Is tuning exported as an input? ([Visibility](languages/rust.md#visibility))
6. **Records.** Does a working input derive serde? Does a genuinely different wire form justify a new wire type? Is there one conversion and a round-trip test against a hand-written fixture? Does `Debug` reach a record, identity, or key? ([Records](languages/rust.md#records), [Debug](languages/rust.md#debug))
7. **Errors.** Is there one error per operation, with context attached once by the layer that knows it? Does a `Display` repeat its source or contain a code? Does panic behaviour match the crate kind? Is every moved refusal listed? Does any fault now pass unrefused? Does any test fix the order of independent faults? ([Errors](languages/rust.md#errors), [Where a refusal sits](languages/rust.md#where-a-refusal-sits))
8. **Purity.** Does a core now read a file, the clock, the environment, or a process? Inside a shell crate, is new logic a named function over values with input and output at the boundaries? ([Purity](languages/rust.md#purity))
9. **Lifetimes and traits.** Does a new lifetime bind a view to its source or merely bundle arguments? Does a new trait have several implementations? Does it invert a dependency the core must not have? Does it exist only for a test double? ([Lifetimes](languages/rust.md#lifetimes), [Traits](languages/rust.md#traits))
10. **Rule-shaped code.** Does a rule force a trait, type, or module that a strong Rust engineer would reject as unnecessary for the domain?
11. **The better shape.** What design would a strong Rust engineer choose here? Why does the proposed design differ? Answer in prose.
12. **Hot paths.** Does per-frame or per-item code allocate once prepared for repeated use? Does it clone to satisfy the borrow checker? Does it grow collections incrementally despite a known size? Does a type added to a hot path have before-and-after instruction counts? ([Performance](languages/rust.md#performance))
13. **Idiom.** Read the change as an experienced Rust reviewer would. Name each place, by file and line, where they would write it differently: tests in a separate file that belong beside their subject, an index loop an iterator states more plainly, a needless clone or allocation, a hand-written loop or helper the standard library or a dependency already provides, `Option` and `Result` handled by matching where a combinator or `?` reads better, a module split or name that hides what the code does. Each is a finding, fixed in the change.

## GDScript

1. **Types.** Does a raw `Variant` or `Dictionary` leave the parser for a consumer? Does a typed declaration from `Variant` lack a preceding `is` test? Is a dictionary passed between functions where a record belongs? Is it read by dot access, or untyped where a typed map fits? Does a sentinel represent absence? ([Typed everything](languages/gdscript.md#typed-everything), [A record is a class](languages/gdscript.md#a-record-is-a-class))
2. **Closed sets.** Does a string or integer represent a closed set? Does a `match` on the project's enum omit a member or an asserting default arm? Does an enum variable lack an explicit default? Does a reported name come from reflection rather than a map? ([Closed sets](languages/gdscript.md#closed-sets))
3. **Invariants.** Can a caller break a class's invariant by writing a field, or build it without its static constructor? ([Invariants](languages/gdscript.md#invariants))
4. **Boundaries.** Is raw parsed data read beyond its parser? Does the code trust a typed export to validate a hand-written file? Does it load any external resource? Does it use a JSON number as `int` without conversion? Does it mutate a shared resource without a copy? Does it assume a deep copy isolates external sub-resources? ([Parsing at boundaries](languages/gdscript.md#parsing-at-boundaries))
5. **Errors.** Does a library function log and return `null`? Is a refusal carried as a `Variant` or printed instead of returned? Is `assert` used on input? Is an engine `Error` dropped? ([Errors](languages/gdscript.md#errors))
6. **Warnings.** Does each new `@warning_ignore` cover one warning on one statement? Does its reason name a constraint the code cannot show? Is a new project-owned addon included in the warning rule? Does an engine upgrade set every new warning? ([Exceptions to a warning](languages/gdscript.md#exceptions-to-a-warning), [Include the project's own addons](languages/gdscript.md#include-the-projects-own-addons), [An engine upgrade reviews the warning list](languages/gdscript.md#an-engine-upgrade-reviews-the-warning-list))
7. **By name.** Is a method, property, or signal reached by name where the type is known? ([No duck typing](languages/gdscript.md#no-duck-typing))
8. **Lifetime.** Who frees each new node? Does the code free it with `queue_free`? After `await`, does a coroutine check the objects it uses? Does it await only objects that outlive it? Does a detached coroutine name its owner? Does a worker access the tree or finish without being joined? Does a static variable have an owner that resets it? Does a static initialiser require the running game? Does opening a `@tool` script cause writes? ([Lifetime, concurrency, and the editor](languages/gdscript.md#lifetime-concurrency-and-the-editor))
9. **Hot paths.** Does per-frame code allocate, look up nodes by path, or read dictionaries? ([Performance](languages/gdscript.md#performance))
10. **The better shape.** What would a strong Godot engineer write here? Why does the proposed design differ? Answer in prose.
11. **Idiom.** Read the change as an experienced Godot engineer would. Name each place, by file and line, where they would write it differently: a loop a built-in or a typed array method states more plainly, a signal or node lookup done by hand where the engine provides it, a helper that duplicates one elsewhere in the project, a name that hides what the code does. Each is a finding, fixed in the change.

How an agent runs a review, and how fixes are checked and landed, is in the [change-review](skills/change-review/SKILL.md) and [change-land](skills/change-land/SKILL.md) skills.
