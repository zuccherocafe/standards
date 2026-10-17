---
name: change-review
description: "Independently review one zucchero✿café change against its brief and the standards' `review.md`. Read the diff and evidence. Check for faults the checks could miss. Write findings in severity order with the smallest fixes, then give a verdict. Use when given a review brief or asked to review a change."
---

# Review a change

You are the one independent reviewer of a change you did not write. Review it against its brief, recorded decisions, the standards' `review.md`, and the language standard. The project's `AGENTS.md` supplies checks and house rules.

## Read

1. Read the review brief. Then read the change's brief, its named decisions, and the writer's report.
2. Before reading code, read the full check log at the tip, change report, public API diff, `refusals.txt`, and line counts. The checks are the acceptance signal. Read that evidence, and do not rerun what it already shows.
3. Read the diff against the base with `git diff`. Then read the change at its tip with `git show`. Read every changed function in full, and the code the change calls.
4. Treat repositories as read-only. To build or run something, first clone into a new scratch folder. Work inside the project's development shell at its resource limits. Stop every process you start by its process ID. Delete the clone afterwards. Never commit.
5. Do not open another reviewer's review.

## Attack

Answer `review.md`'s first five questions. Then answer its general and language questions. Use pass, not applicable, or a finding for each answer. Focus on what a check run cannot show:

- **An undetected fault.** Look for wrongly accepted inputs, untested promises, and values that can become non-finite.
- **An unlisted change.** Look for moved or removed refusals, changed output bytes, removed public items, and remaining readers of removed fields.
- **A fault that real data would expose.** Check "nothing real uses it" claims against the project's real inputs, not only today's examples.
- **A simpler design.** Identify a design with fewer concepts, special cases, or layers that does the same job on real data. State what it removes. Never propose one that is wrong for some correct input.
- **Evidence and its limits.** State what each piece of evidence proves and which claims lack sufficient evidence.

Do not reopen a decision recorded in the brief. Do not request checks of inputs nothing real produces.

## Write findings

Number findings in descending severity order. Include the following for each finding:

- A heading with its severity and the concrete failure in one line.
- The file and line, what goes wrong, and on which input.
- The rule it rests on in `general.md` or the language standard.
- Give the smallest fix. Add no validation, test, or type the finding does not need. Never add a run-time check where a type would suffice.

Every finding is a requested fix in this change: faults, simplifications, idiom, structure, naming and test placement alike. Never defer one to a later pass. The one exception is a finding that asks for new validation, a refusal or a test against an input nothing real produces: call it a note, give the reason, and exclude it from the requested fixes.

Give a short answer to each point in the review brief, stating what you checked and how.

## Give a verdict

End with **land** or **fix first**, naming only findings that must be fixed. Write the review to the named output file. Follow the technical-writing and unslop skills. Add the final line `DONE: final` only when the file is complete.
