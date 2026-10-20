# Engineering rules

These rules apply to every zucchero✿café project, whatever its language. Each language standard in [languages/](languages/) applies these rules to one language and adds language-specific rules. A project's `AGENTS.md` may depart from a rule only as [How strictly a rule applies](#how-strictly-a-rule-applies) allows, and it records each departure with its reason.

## How strictly a rule applies

Each rule gives its kind in one italic word, after its name or at the start of its text: Always, Default, or Weigh.

- **Always.** Every change and every project follows the rule. The only departures are the exceptions the rule itself states, such as the crate whose job needs `unsafe`.
- **Default.** A change follows the rule unless its commit body states a reason to depart from it. A project that departs permanently records the departure and its reason in its `AGENTS.md`.
- **Weigh.** The rule is one value among others. A change follows it where the benefit outweighs the cost, and a review that asks for it states the cost, as [Weigh each fix, then fix the cause](#design) requires.

## Design

**Correctness comes from the design first.** *Always.* Design types and interfaces to make wrong inputs and states impossible, cheaply and in one place, before adding run-time checks. What counts as the right result depends on the input. A real input that a person, file, or tool produces must get the right answer. An input inside a function's stated contract must also get the right answer, even if nothing passes it today. Today's unusual value can become tomorrow's real input. Refuse any other input or fail loudly on it, and never return a silently wrong result. Where a design fix costs more than the harm, refuse the input at the boundary or accept the risk.

**Keep the system coherent.** *Always.* The system's models must fit together. A system is decoherent when its models make sense on their own but do not fit together. Every change then costs more, because work goes into translating between models and guarding against side effects. Adding another mismatched model makes the system more decoherent, and only deliberate work makes it less so. A half-finished transition is the usual cause, so a change that starts a transition finishes it and leaves no old path beside the new one. Before a new phase of work, read the models in that area, check where they overlap, and find any unfinished transition. Decide how much to finish before building on top. [One home per concept](#design) states where each concept lives.

**Design for where the system is going.** *Weigh.* Before a change adds or reshapes a model, name the planned work that will touch it next. Check that the design serves that work rather than closing it off. Weigh this against the cost of designing for work that may never come. A need counts when it is planned, not when it is imaginable.

**Make invalid states unrepresentable.** *Default.* Prefer a type, enumeration, or structure that cannot hold a forbidden domain value over a run-time check. When a run-time check is needed, run it once where the value is made and return the checked value, not a boolean.

**One home per concept.** *Default.* Each concept has one authoritative definition for each genuinely different representation. Never add a copy, a differently structured duplicate, or a parallel table of names. Make every conversion between two representations explicit, named, and checked where it can fail.

**Simplify, never at the cost of correctness.** *Always.* Remove validation, refusals, and tests for inputs that no real user, file, or author produces. At the boundary, keep refusals for mistakes a person can make there. Keep tests for promises that real data relies on. Reject a simpler design that fails on an input that [Correctness comes from the design first](#design) requires to succeed. Weigh what each validation, refusal, or test protects against what it costs, as [Weigh each fix, then fix the cause](#design) requires.

**Parse at the boundary.** *Always.* Parse authored data, files read back, process output, and values from other programs into domain types once, where they enter. Every refusal there names the file, line, and field. Past the boundary, nothing re-checks what a type already proves, and nothing takes the raw form. Three kinds of check after parsing are not redundant. The first checks a newly computed result. The second checks a fact that can change after parsing, such as whether a file still exists with its digest. The third parses bytes that a user supplies to another program.

**A refusal sits where it is cheapest.** *Always.* Place a refusal where it is cheapest and clearest for the author. Refuse as soon as the program has the facts it needs, before expensive work that the refusal would waste. A change may move a refusal earlier or later to meet this rule. A moved refusal must still refuse every input it refused before, and it must stay reachable. The order of independent refusals is not a promise, so no test may pin which independent fault the program reports first.

**Knowledge is data.** *Default.* Facts specific to one input belong in that input's files and their generated output, never in general code or host branches. Such facts include a name, a mapping, or a set of cases one model or file uses. Infer what the files already say by a method that works for any input. Put facts that hold only for one family of inputs in a named preset.

**Generated output is typed and trusted.** *Always.* Build and tool output has typed fields. Parse it once more where it is loaded, as with any other input. Past that point, the program trusts it. Never edit a generated file by hand. Each generated file has exactly one writer.

**Weigh each fix, then fix the cause.** *Always.* When recommending a fix, state who can produce the input that reaches the fault, such as a person making a mistake, a tool, an outside party, or only a crafted file. State what a person would see and what the fix costs. Accepting the risk is always an option. Once a problem is worth fixing, fix it at the root cause, as [skills/principle-fix-root-causes](../skills/principle-fix-root-causes/SKILL.md) requires. Label any workaround as a workaround, state what the real fix would be, and never present the workaround as the recommendation. When the cause is in a dependency, refuse the input at our boundary and name the limit, or accept the risk.

## Checks

A project's checks exist to catch what is broken, not what is different.

**Checks fail only on broken results.** *Always.* A check fails when the result is wrong for a correct input, whatever a change intended. Failures include a crash, a script error, an input that does not load or run, a non-finite value, a missing part, or a violated contract. Set each threshold where a person watching would see something wrong, never at today's value. Explain the threshold in one line.

**A varying measurement never fails the checks.** *Always.* Report any measurement that varies from run to run, such as wall time or memory use, and never fail the checks on it, whatever the maintainer approves. A check on a varying measurement fails on an unlucky run or a busy machine. Measure over repeated runs and report the result with its spread. A test that controls time itself, such as a test with paused time, measures nothing and is an ordinary test. A result that varies because the code is sometimes broken, such as an occasional non-finite value or missing part, is a broken result and must fail the checks.

**Report differences without failing checks.** *Always.* Report changed outputs, measurements, and before-and-after images for a person to read instead of failing the checks on them. The development shell provides [dandegate](../tools/crates/dandegate/) to build the change report. A project declares its report steps in a root `dandegate.toml` and runs its own scripts for its project-specific steps.

**Expected values are authored.** *Always.* Write expected values by hand from the input, a specification, or a law. Never copy them from a run or pin today's output. Exact text or structured output whose shape is the promise, such as help text, a rendered error, or a listing, may use a snapshot. A snapshot counts as authored when a reviewer reads and accepts it in the diff. Numbers and behaviour always use hand-written expected values.

**Catch wrong code in unit tests.** *Default.* Test each function's promises in unit tests with small made-up inputs. Do not write checks of the checks.

**Test the common scenario first.** *Default.* Each component lists its uses, most common first. Each use has an obvious test through the boundary where the component is used. Use the smallest input that reaches that boundary, so scenarios in the checks cost seconds. Unit tests of each function's promises come next, followed by properties of invariants the types cannot express. A review asks which scenario a change touches and whether a test reaches the changed code.

**A test can fail when the code is wrong.** *Always.* Never compute an expected value the way the code under test computes it, call the code under test to produce it, or assert only that the code ran or returned what it returned. Such a test passes whatever the code does. Delete any test that cannot fail when the behaviour breaks, rather than keeping it for coverage, as [skills/principle-test-behavior-not-implementation](../skills/principle-test-behavior-not-implementation/SKILL.md) explains.

**Do not shape production code for its tests.** *Default.* Never widen visibility or add an accessor, parameter, hook, or trait only so a test can reach or replace an internal detail. Test through the public entry point or through the pure function over values that the production code calls. If a promise is untestable without exposing internals, fix the design by separating the logic from input and output.

**Keep a test for a promise.** *Default.* Keep a test when it checks a promise that real data relies on, regardless of who or what found the case. Name the test after that promise. Remove tests for inputs that nothing real produces. When a type makes a failure impossible at compile time, remove its run-time test and keep a test of the static promise, such as a compile-fail test.

**A check above a unit test needs the maintainer's approval.** *Always.* Get the maintainer's approval before adding or widening anything beyond a unit test that can fail the project's checks or adds a process to them. In the request, state what broken behaviour the check catches, why a unit test cannot catch it, and how much running time it adds.

**Explain a failure before fixing it.** *Always.* Before fixing a failure, state what a person would see, which part of the system causes the failure, and which design decision it reveals. Until you can explain the cause, never silence the failure with an exemption, a threshold change, or a per-case patch. A new check states in one line what broken behaviour a person would see without it. If you cannot name that behaviour, do not add the check.

**Describe each check at its real strength.** *Default.* Prefer the compiler and established linters when their check matches a rule. A text scan is a review aid that runs as a report only, unless the maintainer approves it as a failing check. Set no custom size thresholds, such as limits on function length or argument count. Keep a linter's default threshold. When a function needs more inputs than the threshold allows, keep them as separate parameters under a lint exception rather than packing them into a tuple or one-off struct. Place every lint exception on the narrowest item, and state the constraint the code cannot express as its reason. A reason that describes states or modes points to a missing type.

## Toolchain

**Every public project has a development shell.** *Always.* A Lix flake at the repository root provides every tool the project's checks run, at pinned versions. Track both `flake.nix` and `flake.lock` in git, because a flake sees only tracked files. Start from [templates/nix/flake.nix](../templates/nix/flake.nix). Running `nix develop` gives a new machine or an agent's clone every tool the checks need, and the project's checks must pass inside that shell. Keep manual installation steps in the project's `AGENTS.md` as a fallback.

**Pins live in the flake.** *Always.* Each tool is pinned to an exact version and hash. Read a language toolchain from its own pin file, such as `rust-toolchain.toml`, so it has one pin. Fetch a tool built from source by version with fixed hashes. Move a pin only in the same change as the file that checks it. That file may be an `AGENTS.md` version line or a script that refuses another version. The shell and checks must agree.

**Toolchains move often, in small steps.** *Default.* Upgrade each toolchain soon after a release ships. Aim for a dated Rust nightly each month, a monthly nixpkgs revision for Python and its tools, and every stable engine release. Skipping a month is fine, but no toolchain may lag by more than three months. Small, frequent upgrades bring fixes while the affected code is still familiar and keep the number of new lints per upgrade small.

**This repository upgrades first.** *Default.* Upgrade the pins in this repository and in `templates/` first, so every project copies a version that already builds the shared tools. Each project then upgrades in a change of its own.

**One upgrade is one change.** *Default.* Each upgrade change moves one toolchain together with everything coupled to it. Move a Rust nightly with `cargo-public-api`, an engine with its binary, export templates, and binding, or a Python minor release with the target version in every Ruff and Pyrefly configuration. Include no other changes in the commit. Update dependencies in `Cargo.lock` in a separate change on the same schedule.

**Fix new lints instead of suppressing them.** *Default.* When a toolchain upgrade adds a lint, warning, or type check, fix every site it reports in the upgrade change. If the check is wrong for our code, disable it with a reason in the shared template in this repository first. A local lint exception at one site must follow [Describe each check at its real strength](#checks). Remove any workaround and comment that the upgrade makes unnecessary.

**Check and report every toolchain upgrade.** *Default.* Run the full checks and a `dandegate` report with performance steps enabled between the base commit and the upgrade. Explain any output or performance difference that the toolchain caused before building later work on top of it. In the project's checks, [pin-age](../tools/crates/pin-age/) reports each pin over a month old without failing the checks.

**Copy tool configuration, pin shared tools.** *Default.* Cargo, Clippy, rustfmt, cargo-deny, Godot, and the GDScript formatter read only files inside the project, and a clean clone must build on its own. Each project therefore copies the files it uses from [templates/](../templates/) and appends project-specific entries after the shared block. That order keeps the shared block easy to compare against the template. Report any difference from the template without failing the checks. Tools in [tools/](../tools/) take project paths and settings as arguments, so one shared copy serves every project. The project's flake pins this repository in `flake.lock` and puts the tools on `PATH`. Update that pin to this repository's `main` branch at least once a month, or sooner when the project needs a new rule or tool, in a standalone change rather than on every edit here.

**GPU programs are official binaries.** *Always.* A program that draws or plays sound uses the publisher's binary. Examples include a game engine or a 3D suite. Fetch that binary by the publisher's digest and install it unchanged. It then loads the host's graphics and audio libraries and GPU drivers as its publisher built it to. GPU drivers, compositors, and sound servers stay on the host.

**Development state stays in the repository.** *Always.* Put caches, scratch folders, generated files, and tool data written by project commands in a git-ignored folder inside the repository, such as `.cache/`. Keep that folder outside any directory that an engine or build tool imports. Project commands must never write development state to the home directory or to system paths such as `~/.cache` or `~/.local/share`. When a tool defaults to a path outside the repository, redirect it into the repository from the development shell. A package manager's own store and home, such as Nix's store or Cargo's home, stay where the tool keeps them.

**Lix is the supported implementation.** *Default.* Write the flake so other Nix implementations can evaluate it. Fetch inputs as tarballs. Use nothing specific to one implementation. Projects test only with Lix.

## Performance

**Know the hot paths and their budgets.** *Default.* Name the code in each project that runs per frame, per request, or per item of a large input. State each hot path's budget in terms a person notices, such as frame time on the slowest supported device, build or install time, or response time. Record the budget and the measurement device beside the design that the budget constrains.

**Measure before claiming.** *Always.* When claiming that a change is faster, slower, or free, state what you measured and how, following [skills/benchmark-checklist](../skills/benchmark-checklist/SKILL.md). Name the machine, the input, the number of runs, and their spread. To compare revisions, prefer a metric unaffected by other machine activity, such as instructions retired, allocations, or bytes read. To compare against a budget, prefer time. Label any estimate as an estimate and state what measurement will replace it.

**Report performance differences without failing checks.** *Always.* When a change touches a hot path, report its cost before and after in the change report. If a change breaks a budget in a way a person would notice, report it to the maintainer with the measurements. As [A varying measurement never fails the checks](#checks) requires, run-to-run timings never fail the checks. Test a deterministic performance promise, such as zero allocations once warm, as an ordinary check that fails when broken.

**Write clear code first and optimise measured costs.** *Default.* Write the clear version. Measure the hot path. Optimise what the measurement identifies. First remove values that nothing reads, because a value carried and never used costs every call.

## Writing

**Write with the two skills.** *Default.* Prose, documentation, commit messages, and reports follow [skills/unslop](../skills/unslop/SKILL.md) to remove machine-written phrasing and [skills/technical-writing](../skills/technical-writing/SKILL.md) for sentence and document structure. Where either skill disagrees with this file, this file wins. Format code blocks with the project's formatter rather than `technical-writing`'s tab-indented style.

**Use British English.** *Always.* Code, comments, and documentation use British spelling, as in analyse, colour, organisation, and centre. Use American spelling only where an external library or API requires it.

**Comments state constraints.** *Default.* State in a comment only what the code and types cannot express, such as a reason, an external constraint, or an invariant that no type can enforce. A comment such as "never empty", "left first", or "zero when absent" signals a missing type under [Make invalid states unrepresentable](#design).

**Documentation lives beside what it documents.** *Default.* Document each module or crate with its job, what stays outside its scope, its design, and the reasons for that design. Write at whatever length a reader needs to understand the component on its own. Document every public item with its purpose, invariants, and failure modes, and include a runnable example where it helps. Put cross-component and user-facing material in a separate specification that indexes component documentation without repeating it.

**Comments cite no sources.** *Always.* Never name in a code comment the paper, author, talk, or repository an idea came from. That provenance is history rather than something a reader of the code needs. Put sources in the change report or in the project's documentation.

**Comments inside code stay short.** *Always.* A comment inside a function body or a script's code is at most six lines. Blank lines and region markers neither count towards the limit nor end a block. Put a longer explanation in the module's documentation.

**Configuration files carry few comments.** *Default.* In a configuration file, comment only on values whose reason a reader could not guess. Keep instructions for filling in a template inside the skill that copies it, never in the copied file.

**Documentation describes current state only.** *Always.* Do not narrate history, retain descriptions of removed behaviour, or reconstruct the history of TODOs. Avoid historical descriptions such as "removed", "previously", and "no longer". A change's history lives in version control.

**Markdown prose is one paragraph per line.** *Always.* Never hard-wrap prose at a column width. Editors soft-wrap. Lists, tables, and code blocks are unaffected.

**Keep lines within 120 columns.** *Always.* Where a formatter governs a language, its line-length limit applies. Elsewhere, keep lines to at most 120 columns, counting a tab as four columns.

## Skills

**Keep every skill in this repository's `skills/` folder.** *Always.* Every skill that a zucchero✿café project's agents use lives in [skills/](../skills/) and is never copied into a project repository. Updating a skill here then serves every project. Tools that load skills from disk read the symbolic links that the development shell in [templates/nix/flake.nix](../templates/nix/flake.nix) creates to the pinned `standards` input. For other tools, the `Skills` section of the project's `AGENTS.md`, seeded from [templates/AGENTS.md](../templates/AGENTS.md), lists which skill each role reads.

## Version control

**Use `jj` colocated with `git`.** *Always.* Every repository uses `jj` colocated with `git`, so both tools see the same commits and tools that require `git` still work. People and agents use `jj` to inspect and edit history. A temporary clone that an agent makes for a single change may use `git`.

**Land a change as a few self-standing commits.** *Default.* Make each commit one coherent change. Fold a review fix into the commit it repairs rather than adding a fixup commit afterwards, so the commit history shows the finished design rather than the review rounds.

**Land by fast-forwarding `main`.** *Always.* If `main` has moved, rebase the change onto `main`, make sure the full checks pass at the new tip, and fast-forward `main` to the tip. Never land a change with a merge commit.

**Nothing is pushed without the owner's word.** *Always.* An agent must never push to a remote, create a remote, or rewrite a published `main` branch.

**Write scoped, imperative commit messages.** *Default.* Format the subject line in lowercase as `scope: what the commit does`, in at most about 72 characters. Use the single most specific scope for the commit, such as a crate, addon, or folder, matching the names in the project's `AGENTS.md`. Put a sub-area after its area, as in `body: gaze: read the declared eye bones`. When a commit touches several areas, use the primary area's scope, never a list. Start the description with an imperative verb such as `add`, `fix`, or `remove`. Do not use type prefixes such as `feat:` or `fix:`, because the imperative verb already states the kind of change. Version and pin updates are the one exception. State the old and new versions instead of a verb, as in `flake: standards 1b43e835 -> ba256dbd`. Give every non-trivial commit a body with one line per paragraph. State why you made the change and what you deliberately left out. Note anything a reader of the diff might miss, such as a moved refusal or a format change. Write the body so it stands on its own without pointing to a private document or review.

A subject that lists areas and summarises the whole change:

```text
parser, cli, docs: the config file's new include directive, resolved relative to the including file, with cycles refused and the help text updated
```

becomes a scoped subject and a body:

```text
config: add an include directive

Large configurations repeat the same blocks across files. An include line now reads another file in place, resolved relative to the file that includes it.

The parser refuses an include cycle and names every file in the cycle. The help text documents the directive. Nothing else in the format changes.
```

**Each commit passes its checks.** *Always.* Each commit passes the quick checks that the project's `AGENTS.md` names for every commit. The full checks pass at the change's tip and again after every rebase.

**Omit author and tool attribution trailers.** *Always.* Include no `Co-Authored-By` line or other co-author, tool, or session trailer in commit messages, and name no author or tool in comments or documentation. The person who submits a change takes full responsibility for it. A tool trailer shifts part of that responsibility to a tool and adds only advertising to the log.
