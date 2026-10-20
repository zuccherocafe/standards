---
name: change-review
description: "Independently review one zucchero✿café change against its brief and the standards' `review.md`. Read the diff and evidence. Check for faults the checks could miss. Write must-fix and should-fix findings with the smallest fixes, then give a verdict. Use when given a review brief or asked to review a change."
---

# Review a change

You are the one independent reviewer of a change you did not write. Review it against its brief, recorded decisions, the standards' `review.md`, and the language standard. The project's `AGENTS.md` supplies checks and house rules.

## Read

1. Read the review brief. Then read the change's brief, its named decisions, and the writer's report.
2. Before reading code, read the full check log at the tip and any other evidence the brief picked, such as the change report, public API diff, `refusals.txt`, and line counts. The checks are the acceptance signal. Read that evidence, and do not rerun what it already shows.
3. Read the diff against the base with `git diff`. Then read the change at its tip with `git show`. Read every changed function in full, and the code the change calls.
4. Treat repositories as read-only. To build or run something, first clone into a new scratch folder. Work inside the project's development shell at its resource limits. Stop every process you start by its process ID. Delete the clone afterwards. Never commit.
5. Do not open another reviewer's review.

## Attack

Answer `review.md`'s first questions. Then answer its general and language questions. Use pass, not applicable, or a finding for each answer. A pass needs no prose. Focus on what a check run cannot show:

- **An undetected fault.** Look for wrongly accepted inputs, untested promises, and values that can become non-finite.
- **An unlisted change.** Look for moved or removed refusals, changed output bytes, removed public items, and remaining readers of removed fields.
- **A fault that real data would expose.** Check "nothing real uses it" claims against the project's real inputs, not only today's examples.
- **A simpler design.** Identify a design with fewer concepts, special cases, or layers that does the same job on real data. State what it removes. Never propose one that is wrong for an input that must get the right answer ([Correctness comes from the design first](../../rules/general.md#design)).
- **Evidence and its limits.** State what each piece of evidence proves and which claims lack sufficient evidence.

Do not reopen a decision recorded in the brief. Do not request checks of inputs nothing real produces.

## Write findings

Each finding is one of two kinds:

- **Must-fix.** A real input gives a wrong result, or a stated promise breaks. The finding blocks landing, and the writer fixes it in the change.
- **Should-fix.** The fix is worth making, but no real input gives a wrong result without it. The writer makes it or declines it with a reason. A declined finding is dropped for good.

Number the must-fix findings first, then the should-fix ones. Include the following for each finding:

- A heading with its kind and the concrete failure in one line.
- The file and line, and what goes wrong.
- The real input that reaches it, or the stated promise it breaks, and what a person would see. Or, for a should-fix finding, why it is not must-fix. If you can name a real input it affects, it is must-fix.
- The rule it rests on in `general.md` or the language standard.
- The smallest fix and what it costs ([Weigh each fix, then fix the cause](../../rules/general.md#design)). Add no validation, test, or type the finding does not need. Prefer a type to a run-time check unless the type costs more than the harm ([Correctness comes from the design first](../../rules/general.md#design)), and never re-check what a type already proves.

When the same fault appears in other places, triage each one now: a real input reaching it makes it must-fix in this change, and otherwise drop it. Nothing is deferred to a later change.

Give a short answer to each point in the review brief, stating what you checked and how.

## Give a verdict

End with **land** when no finding is must-fix, or **fix first**, naming the must-fix findings. Write the review to the named output file. Follow the technical-writing and unslop skills. Add the final line `DONE: final` only when the file is complete.
