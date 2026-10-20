---
name: change-land
description: "As coordinator, check a zucchero✿café change's review fixes once, decide what remains, and land it on `main` by fast-forward. If `main` has changed, rebase and rerun full checks. Use when the writer reports review fixes complete or a reviewed change is ready to land."
---

# Check a change's fixes and land it

The coordinator lands changes. When one change runs at a time, `main` stays unchanged during that work, so the writer's evidence at the final tip also supports landing. The project's `AGENTS.md` supplies checks and house rules.

## Check the fixes narrowly

A change gets one review and one check of its fixes. After the writer fixes the review's findings, read only the fixes, against the findings they answer. Do not review the whole change again.

1. List the review's must-fix and should-fix findings, and the writer's declines.
2. Read each fix commit's diff. For each finding, record **OK** or the remaining fault with its file and line.
3. Read every decline. If its reason is wrong, such as when a real input reaches the finding, mark the finding must-fix.
4. Check that each fix follows [Fix a review's findings](../change-write/SKILL.md#fix-a-reviews-findings). Identify any growth in production code and check whether a smaller design avoids it.
5. Check that nothing outside the findings changed. Where the brief picked a change report or public API diff, it must show only changes explained by the fixes.
6. Check that the evidence comes from the new tip. Full checks must pass there. Any change report or comparison the brief picked must come from runs after the last fix.

## Decide what remains

No second review or check follows. After the check, triage what remains and decide.

- If no must-fix finding remains, read the declines again and proceed to [Decide whether to rebase](#decide-whether-to-rebase).
- Must-fix findings that remain after the check suggest the design is wrong. Either send the writer the few that matter and read their fixes yourself, or decide on a redesign.
- Ask the maintainer only when a redesign changes a format, a type, an API, or a dependency.

Never fix the change's code yourself.

## Decide whether to rebase

Compare the change's base with the current `main`.

- If `main` is unchanged, use the writer's full-check evidence at the tip for landing. Proceed to the landing steps.
- If `main` has changed, rebase onto it in a scratch clone. Resolve a conflict only where it is mechanical. A mechanical resolution keeps both sides' lines, merges imports, or follows a renamed path. If resolution needs a judgement about intent, stop. Ask the maintainer and name the files and both intents.
- After rebasing, repeat the fresh-tree preparation required by the project's `AGENTS.md`. Run full checks at the rebased tip, and the change report if the brief picked one. If anything fails, stop. Report the failure to the writer.

## Land by fast-forward

1. Bring the change's final branch into the main repository.
2. Fast-forward `main` to the final branch with `git -C <main checkout> merge --ff-only <branch>`. If git refuses the fast-forward, return to the rebase step because `main` has changed.
3. Delete the change's branch. If you made a scratch clone, delete it.
4. Leave the main checkout clean and on `main`. Never push. Never create a remote. Leave nothing running.

## Write the report

Write a short report that follows the technical-writing and unslop skills. Include these items:

- The new `main` revision.
- The narrow check's items with their results, and each decline with whether it stands.
- Whether a rebase happened, how each conflict was resolved, and the full checks' result and time at the landed tip.
- The change report's verdict, where the brief picked one.
