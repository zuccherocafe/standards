# Engineering rules

These rules apply to every zucchero✿café project, whatever its language. A language standard, such as [languages/rust/](languages/rust/), applies these rules to that language and adds language-specific rules. A project's `AGENTS.md` may depart from either standard. It records each departure with its reason.

## Design

**Make invalid states unrepresentable.** Make a value the domain forbids impossible to construct, rather than checking for it later. Prefer a type, enumeration, or structure that cannot hold an invalid value over a run-time check. When a check is needed, run it once where the value is made. Return the checked value, not a yes or no.

**One home per concept.** Each concept has one authoritative definition for each genuinely different representation. Never add a copy, a differently structured duplicate, or a parallel table of names. A conversion between two representations is explicit, named, and checked where it can fail.

**Simplify, never at the cost of correctness.** Remove validation, refusals, and tests for inputs that no real user, file, or author produces. Keep refusals of mistakes a person can make at the boundary where they make them. Keep tests of promises real data relies on. Reject a simpler design that is wrong for some correct input.

**Parse at the boundary.** Parse authored data, files read back, process output, and values from other programs into domain types once, where they enter. Every refusal there names the file, line, and field. Past the boundary, nothing re-checks what a type already holds, and nothing takes the raw form. Three checks are not redundant. The first checks a newly computed result. The second checks a fact that can change after parsing, such as whether a file still exists with its digest. The third parses bytes a user supplies to another program.

**Knowledge is data.** Facts specific to one input belong in that input's files and their generated output, never in general code or host branches. Such facts include a name, a mapping, or a set of cases one model or file uses. Infer what the files already say by a method that works for any input. Put facts that hold only for one family of inputs in a named preset.

**Generated output is typed and trusted.** Build and tool output has typed fields. Parse it once more where it is loaded, as with any other input. Past that point, the program trusts it. Never edit a generated file by hand. Each generated file has exactly one writer.

**Fix the cause.** Recommend the fix where the fault is. Make the fix there. Label a workaround as a workaround and state what the real fix would be. Never present the workaround as the recommendation.

## Checks

A project's checks exist to catch what is broken, not what is different.

**Checks fail only on broken results.** A check fails when the result is wrong for a correct input, whatever a change intended. Failures include a crash, a script error, an input that does not load or run, a non-finite value, a missing part, or a violated contract. Set each threshold where a person watching would see something wrong, never at today's value. Explain the threshold in one line.

**Report differences without failing checks.** Report changed outputs, measurements, and before-and-after images for a person to read. Nothing is pinned to today's output, so no expectation is derived from that output. The change report is [dandegate](../tools/crates/dandegate/), which the development shell provides. A project declares the steps it runs in a root `dandegate.toml`, and its own scripts do the work particular to it.

**Expected values are authored.** A check compares with values written by hand from the input's source, a specification, or a law, never copied from a run. Exact text or structured output whose shape is the promise may use snapshots. Examples include help text, a rendered error, or a listing. A snapshot counts as authored when a reviewer reads and accepts it in the diff. Numbers and behaviour keep hand-written expectations.

**Wrong code is caught where it is written.** Unit tests state what a function promises, on small made-up inputs. "The output equals what it was last time" is not a unit test. Do not write checks of the checks.

**Test the common scenario first.** Each component lists its uses, most common first. Each use has an obvious test through the boundary where the component is used. Use the smallest input that reaches that boundary, so scenarios in the checks cost seconds. Unit tests of each function's promises come next, followed by properties of invariants the types cannot express. A review asks which scenario a change touches and whether a test reaches the changed code.

**A test can fail when the code is wrong.** Never compute a test's expected value the way the code computes it, call the code under test to produce it, or assert only that the code ran or returned what it returned. Such a test passes whatever the code does. Each test states a promise from the input's source, a specification, or a law that a wrong implementation would break. Delete a test that cannot fail, rather than keeping it as coverage.

**Code is not shaped for its tests.** Never widen visibility, add an accessor, a parameter, a hook, or a trait only so a test can reach or replace something. Test through the public entry point or the plain function over values the production code already calls. When a promise cannot be tested without such a change, the design is wrong: separate the logic from its inputs and outputs instead.

**Keep a test for a promise.** Keep a test when it states a promise real data relies on, regardless of who or what found the case. Name the test for that promise. Remove tests of inputs nothing real produces. When a type makes a failure impossible, remove its run-time test. Keep a test of the static promise, such as a compile-fail test.

**A check above a unit test needs the maintainer's approval.** Beyond a unit test, anything that can fail the project's checks or adds a process to them needs approval before addition or widening. The request says what broken thing the check catches, why a unit test cannot catch it, and how much running time it adds.

**Explain a failure before fixing it.** Say what a person would see, which system part produces the failure, and which design decision the failure reveals. Until the cause is explained, never fix the failure with an exemption, a threshold change, or a per-case patch. A new check states in one line what broken thing a person would see without it. If the check cannot state that failure, do not add it.

**Describe each check at its real strength.** Prefer the compiler and established linters where their meaning matches a rule. A text scan is a review aid and reports only until the maintainer approves it as a check. Set no custom size thresholds, such as a limit on function length or argument count. A linter's default threshold stays, and a function whose required inputs exceed it takes them under a lint exception. Required inputs stay inputs, never packed into a tuple or one-off structure to satisfy a count. Put a lint exception on the narrowest item. Its reason names a constraint the code cannot show. A reason that describes states or modes indicates a missing type.

## Toolchain

**Every public project has a development shell.** A Lix flake at the repository root provides every tool the project's checks run, at the project's pinned versions. Track both `flake.nix` and `flake.lock` in git, since a flake sees only tracked files. Use [templates/nix/flake.nix](../templates/nix/flake.nix) as the starting point. The command `nix develop` gives a new machine or an agent's clone what the checks need. The project's checks pass inside that shell. Keep manual installation steps in the project's `AGENTS.md` as the fallback.

**Pins live in the flake.** Each tool is pinned to an exact version and hash. Read a language toolchain from its own pin file, such as `rust-toolchain.toml`, so it has one pin. Fetch a tool built from source by version with fixed hashes. Move a pin only in the same change as the file that checks it. That file may be an `AGENTS.md` version line or a script that refuses another version. The shell and checks must agree.

**Toolchains move often, in small steps.** Each project moves its toolchains to new releases soon after they ship: a dated Rust nightly at least monthly, the nixpkgs revision that provides Python and its tools at least monthly, and an engine at every stable release. No toolchain is more than three months behind. Small, frequent moves bring fixes while the affected code is familiar, and keep each move's new lints few.

**Standards moves first.** This repository moves its own pins and its templates' first, so a project copies a version that has already built the shared tools. Each project then follows in a change of its own, as it does for its standards pin.

**One upgrade is one change.** It moves one toolchain with everything coupled to it: a Rust nightly with `cargo-public-api`; an engine with its binary, export templates and binding; a Python minor release with the target version in every Ruff and Pyrefly configuration. Nothing else rides in the change. Updating the dependencies in `Cargo.lock` is a change of its own, on the same schedule.

**A new lint is fixed, not allowed.** Where the new toolchain adds a lint, warning or type check, the upgrade change fixes every site it reports. If the check is wrong for our code, it is turned off in the shared template with its reason, here first. A new allow at one site needs the reason any exception needs. A workaround the release makes unnecessary is removed with its note.

**The upgrade shows what moved.** The change carries the full checks and a dandegate report from the base to the upgrade, with the performance steps selected. A difference the toolchain caused is explained before later work depends on it.

**Checked by:** review of the upgrade change against these rules; the project's checks, where [pin-age](../tools/crates/pin-age/) prints a line for each pin over a month old and fails on none.

**Copy tool configuration, pin shared tools.** Cargo, Clippy, rustfmt, cargo-deny, Godot, and the GDScript formatter read only files inside the project, and a clean clone must build alone. So a project copies the files in [templates/](../templates/) it uses and adds its own entries after the shared ones, which keeps the shared part comparable with the latest template. A difference from the template is reported, never a failed check. A tool in [tools/](../tools/) reads the project's files and takes project details as arguments, so one copy serves every project. The development shell takes this repository as a flake input pinned in `flake.lock` and puts the tools on `PATH`. Whenever this repository's `main` moves, each project moves its pin to it in a change of its own, so every project reads the latest standards.

**GPU programs are official binaries.** A program that draws or plays sound uses the publisher's binary. Examples include a game engine or a 3D suite. Fetch that binary by the publisher's digest and install it unchanged. It then loads the host's graphics and audio libraries and GPU drivers as its publisher built it to. GPU drivers, compositors, and sound servers stay on the host.

**Development state stays in the repository.** Caches, scratch folders, generated files, and tool data that a project's commands write go in a git-ignored folder inside the repository, such as `.cache/`, outside any folder an engine or build tool imports. Commands never write development state to the home directory or to system-wide places such as `~/.cache` or `~/.local/share`. Where a tool chooses such a place itself, the development shell points it into the repository. A package manager's own store and home, such as Nix's store or Cargo's home, stay where the tool keeps them.

**Lix is the supported implementation.** Write the flake so other Nix implementations can evaluate it. Fetch inputs as tarballs. Use nothing specific to one implementation. Projects test only with Lix.

## Performance

**Know the hot paths and their budgets.** Each project names code that runs per frame, per request, or per item of a large input. It names each path's budget in terms a person notices. Examples include frame time on the slowest supported device, build or install time, and response time. Record the budget beside the design it constrains, with the device used to measure it.

**Measure before claiming.** A claim that a change is faster, slower, or free says what was measured and how. It names the machine and input, the number of runs, and their spread. To compare revisions, prefer a measure unaffected by other machine activity, such as instructions retired, allocations, or bytes read. To compare against a budget, prefer time. Label an estimate as an estimate and state what would replace it.

**Report performance differences without failing checks.** Every change that touches a hot path reports its cost before and after, beside the report of other changes. Raise a failure only for a broken budget that a person would notice. Send that failure and its evidence to the maintainer. A timed check enters the project's checks only with the maintainer's approval. It must fail on broken results, not on a busy machine.

**Write clear code first and optimise measured costs.** Write the clear version. Measure the hot path. Optimise what the measurement identifies. First remove values that nothing reads, because a value carried and never used costs every call.

## Writing

**Write with the two skills.** Prose, documentation, commit messages, and reports follow [skills/unslop](../skills/unslop/SKILL.md) and [skills/technical-writing](../skills/technical-writing/SKILL.md). The first removes patterns of machine-written text. The second governs sentences and document structure. Where either skill disagrees with this file, this file wins. Use British spelling and one paragraph per Markdown line. Use the project's formatter rather than the skill's tab-indented code snippets.

**Use British English** in code, comments, and documentation, as in analyse, colour, organisation, and centre. Use American spelling only where an external library or API requires it.

**Comments state constraints.** A comment states something the code cannot show, such as a reason, constraint, or invariant the types cannot express. An invariant that a type could enforce instead indicates that the type admits invalid values. Examples include "never empty", "left first", and "zero when absent".

**Documentation lives beside what it documents.** A module's or crate's documentation states its job, what it never knows, its design, and the reasons for that design. Use whatever length the design needs, so a reader can understand the component from the component itself. Every public item's documentation states its purpose, invariants, and failure modes. Include a runnable example where it helps. A separate specification holds material that spans components and material a user of the project reads. It indexes component documentation rather than repeating it.

**Comments cite no sources.** A comment never names the paper, author, talk or repository an idea came from. Where it came from is history, not something a reader of the code needs; it goes in the change's report or the project's documentation.

**Comments inside code stay short.** A comment inside a function body or a script's code is at most six lines. Blank lines and region markers neither count towards the limit nor end a block. Put a longer explanation in the module's documentation.

**Configuration files carry few comments.** A configuration file comments only on a value whose reason a reader couldn't guess. Instructions for filling a template live in the skill that copies it, and a copied file keeps none of them.

**Documentation describes current state only.** Do not narrate history, retain descriptions of removed behaviour, or reconstruct the history of TODOs. Avoid historical descriptions such as "removed", "previously", and "no longer". A change's history lives in version control.

**Markdown prose is one paragraph per line.** Never hard-wrap prose at a column width. Editors soft-wrap. Lists, tables, and code blocks are unaffected.

**Line length.** Where a formatter governs a language, it decides. Elsewhere, lines are at most 120 columns. A tab counts as four columns.

## Skills

**The standards' skills folder is the one source.** Every skill a zucchero✿café project's agents use lives in the standards' [skills/](../skills/) folder, never copied into a project. A role's changes then reach every project at once. A tool that loads skills reads links the development shell creates to the pinned `standards` input ([templates/nix/flake.nix](../templates/nix/flake.nix)). For other tools, the Skills section of the project's `AGENTS.md` names the skill each role reads ([templates/AGENTS.md](../templates/AGENTS.md)).

## Version control

**jj, colocated with git.** Every repository uses jj colocated with git, so jj and git see the same commits and tools that need git still work. A person or an agent uses jj for history. A scratch clone an agent makes for one change may use git.

**A change lands as few, self-standing commits.** Each commit is one coherent piece. Fold a review fix into the commit it fixes rather than adding a commit after it, so the history reads as the change was meant, not as it was made.

**Landing is a fast-forward of `main`.** A change is rebased onto `main` if `main` moved, its full checks pass at the new tip, and `main` then moves to it. Nothing is merged with a merge commit.

**Nothing is pushed without the owner's word.** An agent never pushes, never creates a remote, and never rewrites a published `main`.

**Message.** A reader scanning the log looks first for which part of the project changed, then for what the commit does. The subject is lowercase `scope: what the commit does`, in at most about 72 characters. The scope is the one most specific area the commit belongs to, such as a crate, an addon, or a folder, named the way the project's `AGENTS.md` lists them. A sub-area follows its area, as in `body: gaze: read the declared eye bones`. A commit that touches several areas takes the scope of the area its change belongs to, never a list. The description is in the imperative, as in `add`, `fix`, or `remove`, so it reads as what applying the commit does. A version or pin change is the one exception: it states the old and new versions instead of a verb, as in `flake: standards 1b43e835 -> ba256dbd`. Every commit that is not trivial has a body, one line per paragraph. The body says why the change was made, what a reader of the diff might miss, such as a moved refusal or a changed format, and what was deliberately left out. It stands on its own and never points to a private document or a review. A type prefix such as `feat:` or `fix:` is not used, since the description already says what kind of change it is.

A subject that lists areas and summarises the whole change:

```text
parser, cli, docs: the config file's new include directive, resolved relative to the including file, with cycles refused and the help text updated
```

becomes a scoped subject and a body:

```text
config: add an include directive

Large configurations repeat the same blocks across files. An include line now reads another file in place, resolved relative to the file that includes it.

An include cycle is refused, naming every file in the cycle. The help text documents the directive. Nothing else in the format changes.
```

**Each commit passes its checks.** Each commit passes the quick checks that the project's `AGENTS.md` names for every commit. The full checks pass at the change's tip and again after every rebase.

**No attribution.** Commit messages carry no `Co-Authored-By` line or other co-author, tool, or session trailer, and code comments and documentation name no author or tool. The person who submits a change is responsible for all of it, and a tool trailer passes part of that to the tool while adding nothing to the log but an advertisement.
