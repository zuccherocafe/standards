---
name: change-review
description: "Independently review one zucchero✿café change against its brief and the standards' `review.md`. Read the diff and evidence. Check for faults the checks could miss. Write must-fix and should-fix findings with the smallest fixes, then give a verdict. Use when given a review brief or asked to review a change."
---

# Review a change

You are the one independent reviewer of a change you did not write. Review it against its brief, recorded decisions, the standards' `review.md`, and the language standard. The project's `AGENTS.md` supplies checks and house rules.

## Read the brief, evidence, and diff

1. Read the review brief. Then read the change's brief, its named decisions, and the writer's report. In a brief review of a design-sized brief, run the [design-read](../design-read/SKILL.md#sample-the-area-in-a-brief-review) skill's check and sample.
2. Before reading code, read the full check log at the tip and any other evidence the brief picked, such as the change report, public API diff, `refusals.txt`, and line counts. The checks are the acceptance signal. Do not rerun what the evidence already shows.
3. Read the diff against the base with `git diff`. Then read the change at its tip with `git show`. Read every changed function in full, and the code the change calls.
4. Treat repositories as read-only. To build or run something, first clone into a new scratch folder. Work inside the project's development shell at its resource limits. Stop every process you start by its process ID. Delete the clone afterwards. Never commit.
5. Do not open another reviewer's review.

## Answer the review questions and check what tools miss

Answer every question in [`review.md`](../../rules/review.md), starting with its `First` section. A pass needs no prose. Focus on what a check run cannot show:

- **An undetected fault.** Look for wrongly accepted inputs, untested promises, and values that can become non-finite.
- **An unlisted change.** Look for moved or removed refusals, changed output bytes, removed public items, and remaining readers of removed fields.
- **A fault that real data would expose.** Check "nothing real uses it" claims against the project's real inputs, not only today's examples.
- **A simpler design.** Identify a design with fewer concepts, special cases, or layers that does the same job on real data. State what it removes. Never propose one that is wrong for an input that must get the right answer ([Correctness comes from the design first](../../rules/general.md#design)).
- **Evidence and its limits.** State what each piece of evidence proves and which claims lack sufficient evidence.

Do not reopen a decision recorded in the brief. Do not request checks of inputs nothing real produces.

## Write findings

Each finding is one of two kinds:

- **Must-fix.** A real input gives a wrong result, or a stated promise breaks. The finding blocks landing, and the writer fixes it in the change.
- **Should-fix.** The fix is worth making, but no real input gives a wrong result without it. The writer makes the fix unless it costs more than a few lines or changes the design, or declines it with its reason and cost. A declined finding is dropped for good.

Number the must-fix findings first, then the should-fix ones. Include the following for each finding:

- A heading with its kind and the concrete failure in one line.
- The file and line, and what goes wrong.
- The real input that reaches it, or the stated promise it breaks, and what a person would see. Or, for a should-fix finding, why it is not must-fix. If you can name a real input it affects, it is must-fix.
- The rule it rests on in `general.md` or the language standard.
- The smallest fix and what it costs ([Weigh each fix, then fix the cause](../../rules/general.md#design), [Make invalid states unrepresentable](../../rules/general.md#design), [Parse at the boundary](../../rules/general.md#design)). Add no validation, test, or type the finding does not need.

When the same fault appears in other places, triage each one now. If a real input reaches a site, mark that site must-fix in this change. Otherwise, drop it. Never defer a finding to a later change.

Answer each point in the review brief. State what you checked and how.

## Give a verdict

If no finding is must-fix, end with **land**. Otherwise, end with **fix first** and name the must-fix findings. Write the review to the named output file. Follow the technical-writing and unslop skills. Add the final line `DONE: final` only when the file is complete.
