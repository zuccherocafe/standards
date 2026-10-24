---
name: change-write
description: "Write one change from a brief in a zucchero✿café project. Work in a scratch clone with one concept per commit. Run quick checks per commit and full checks at the tip. Keep the evidence the brief picks. Finish with a DONE or QUESTION report. Use when given a brief to implement or when resuming to fix findings on your own change."
---

# Write a change from its brief

Each change has a brief based on the standards' `templates/BRIEF.md`. You build the change. Someone else reviews and lands it. The project's `AGENTS.md` supplies checks, the development shell, resource limits, and matters this skill leaves to the project. Where the brief disagrees with `AGENTS.md` or this skill, the brief wins for that change.

## Before you write code

1. Read the entire brief. Then read everything under "Read first": the standards sections, decisions to implement, and earlier review findings to preserve. Stop if the brief's `Concord note:` line is missing, says `none` where the [concord](../concord/SKILL.md#when-a-concord-note-is-due) trigger applies, or names a note that fails that skill's check.
2. Read the project's `AGENTS.md` and its standards: `general.md`, the language standard, and `review.md`.
3. Open every site the brief names. Find each by its symbol, not its line, since other changes land while you work.
4. Do not reopen a decision under "Decided". If the brief leaves a required question of intent unanswered, stop. Report `QUESTION:` rather than guess.

## Work in a scratch clone

1. Clone the repository into a new folder outside the project's checkout, on disk rather than in a RAM-backed `/tmp`. Create the branch named by the brief.
2. If the brief names a base revision, use it. Otherwise, base the branch on the current `main`.
3. Prepare the clone as the project's `AGENTS.md` requires, including one-off commands and machine-local inputs. Copy those inputs into the clone. Never write to the main checkout's caches, build outputs, or ignored folders.
4. Run every command inside the project's development shell with `nix develop -c <command>`. Use the lowered priority and job counts set by `AGENTS.md`.

## Commit one concept at a time

1. Follow the brief's "Do" list. Make one commit per concept in an order that keeps every commit's quick checks passing. Commit the linked concord note and keep it current as [Keep the concord note current](../concord/SKILL.md#keep-the-concord-note-current) requires.
2. Run the quick checks before every commit. Run the full checks at the change's tip after its last commit, and again after every rebase.
3. Stage files by explicit path. Modify only files assigned to the change. In a shared file such as `AGENTS.md` or a README, change only your lines.
4. Write each message as [general.md's Version control](../../rules/general.md#version-control) says, with no attribution trailer.
5. Follow [Checks](../../rules/general.md#checks). Add or widen a check above a unit test only when the brief records the maintainer's approval. Never pin today's value or a count in a check; no approval allows it.
6. Move a refusal only as [A refusal sits where it is cheapest](../../rules/general.md#design) says.
7. Give every property test a fixed seed and disable failure persistence. Rerunning must reproduce a failure, and no test action may leave a file in the repository.

## Record the reasons for decisions

Put the reason for each decision under the brief's "Decided", and for each question you settle that a maintainer would need, where a maintainer would look when wondering about it: the documentation beside the rule or code it shaped ([Documentation lives beside what it documents](../../rules/general.md#writing)), or the commit body for a one-off ([Write scoped, imperative commit messages](../../rules/general.md#version-control)). A decision not to do something goes in the documentation of what it would have changed.

## Keep the evidence

Write the evidence the brief's "Evidence" section picks into the folder it names. Without a pick, the evidence is the check logs: the quick checks' result for each commit and the full checks' log at the tip. Keep the evidence an Always rule requires, whatever the brief picks, such as a hot path's cost before and after ([Report performance differences without failing checks](../../rules/general.md#performance)). A brief may also pick any of the following:

- A report of differences from the base, using the project's change report where available. Every written file remains equal except where the brief permits a difference.
- The public API before and after, for a change to a library crate's public items.
- For a change to Rust, the mutation report against the base, with each listed mutant judged ([Mutation report](../../rules/languages/rust/tests.md#mutation-report)).
- For a change to a hot path, the cost before and after, measured as the project's `AGENTS.md` says.
- Where a binary's behaviour must not change, a comparison of the base and tip binaries on the same inputs.
- In `refusals.txt`, every added, removed, changed, or moved refusal with its reason.
- Production and test line counts per commit.

## Review your own change

Before reporting, read the whole change as its reviewer will: the full diff from the base and the tree of every folder it adds or changes. Answer `review.md`'s questions and check the language standard's rules for each file, including [module layout](../../rules/languages/rust/design.md#modules), [Scenarios first](../../rules/languages/rust/tests.md#scenarios-first) (`tests/scenarios.rs`), [A test can fail when the code is wrong](../../rules/general.md#checks), test placement, and naming. Fix everything you find in the change's own lines, with the checks passing again. In the report, list what you found and fixed this way.

## Fix a review's findings

When asked to resume work on review findings, work only on those findings. Fix every must-fix finding. Fix each should-fix finding unless the fix costs more than a few lines or changes the design. When you decline a should-fix finding, state the reason and the cost of the fix in the report. A declined finding is dropped for good. Fix each cause with the smallest required change. Keep production code the same size or smaller where possible. If growth is unavoidable, explain why in the report. Run the full checks at the new tip. Refresh the evidence.

## Clean up

1. Stop every process you started, using its process ID. Never match processes by name or pattern.
2. Fetch the branch back into the project's main repository. Never modify its `main`. Never push. Never create a remote.
3. Copy the evidence out. Then delete the scratch clone. Leave nothing running.

## Write the report

End with a short report following [Writing](../../rules/general.md#writing), with the answer first. Include the following information:

- The commits, each with one line on what it changes.
- The full checks' result and time at the tip, and the evidence paths.
- Every measure that moved the wrong way, and anything you did not verify.
- Each departure from the brief, with its reason, and how you settled each of the brief's open questions.
- What your own review found and fixed.
- Each declined should-fix finding, with its reason and what the fix would cost.

When the change is complete, end with `DONE: <one line>`. When stopping to ask a question, end with `QUESTION: <the question>`.
