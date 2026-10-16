---
name: change-land
description: "As coordinator, verify a zucchero✿café change's review fixes against its evidence and land it on `main` by fast-forward. If `main` has changed, rebase and rerun full checks. Use when the writer reports review fixes complete or a reviewed change is ready to land."
---

# Check a change's fixes and land it

The coordinator lands changes. No separate landing role is needed. With one change in progress at a time, `main` stays unchanged during that work. The writer's evidence at the final tip therefore also supports landing. The project's `AGENTS.md` supplies checks and house rules.

## Check the fixes narrowly

After the writer fixes a review's findings, read only the fixes, against the findings they answer. Do not review the whole change again.

1. List the required findings from the reviews and the decision selecting them for correction.
2. Read each fix commit's diff. For each finding, record **OK** or the remaining fault with its file and line.
3. Check that each fix addresses its cause with the smallest required change. Check that production code stays the same size or smaller where possible. Identify growth and whether a smaller design avoids it.
4. Check that nothing outside the findings changed. The change report and public API diff must show only changes explained by the fixes.
5. Check that the evidence comes from the new tip. Full checks must pass there. The change report and any comparisons must come from runs after the last fix.

If a fix is wrong, send the writer the unresolved finding. Never fix the change's code yourself.

## Decide whether to rebase

Compare the change's base with the current `main`.

- If `main` is unchanged, use the writer's full-check evidence at the tip for landing. Proceed to the landing steps.
- If `main` has changed, rebase onto it in a scratch clone. Resolve a conflict only where it is mechanical. A mechanical resolution keeps both sides' lines, merges imports, or follows a renamed path. If resolution needs a judgement about intent, stop. Ask the maintainer and name the files and both intents.
- After rebasing, repeat the fresh-tree preparation required by the project's `AGENTS.md`. Run full checks and the change report at the rebased tip. If anything fails, stop. Report the failure to the writer.

## Land by fast-forward

1. Bring the change's final branch into the main repository.
2. Fast-forward `main` to the final branch with `git -C <main checkout> merge --ff-only <branch>`. If git refuses the fast-forward, return to the rebase step because `main` has changed.
3. Delete the change's branch. If you made a scratch clone, delete it.
4. Leave the main checkout clean and on `main`. Never push. Never create a remote. Leave nothing running.

## Report

Following the technical-writing and unslop skills, report the following in a few lines:

- The new `main` revision.
- The narrow check's items with their results.
- Whether a rebase happened, how each conflict was resolved, and the full checks' result and time at the landed tip.
- The change report's verdict.
