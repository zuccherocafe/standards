---
name: principle-attack-the-premise
description: "Apply when two or more fixes that share one premise have failed the same check. Write the premise down and test it directly before you write another fix that assumes it."
---

# Attack the premise

When two or more fixes that share one premise have failed the same check, suspect the premise, not the fixes. Each failed fix is evidence against the belief it assumed. A third fix on the same premise spends effort where the evidence already shows the cause does not lie.

Follow these steps before writing another fix:

- **Write the premise down.** State the one sentence that every failed fix assumed.
- **Test the premise before the next fix.** Make a repeatable observation that would come out differently if the premise were false. Check how the failures are distributed, not only how large they are.
- **Follow the evidence.** If the observation contradicts the premise, look for what the failures share that the premise did not predict. If it confirms the premise, ask the next "why" as in [Fix root causes](../principle-fix-root-causes/SKILL.md), because the earlier fixes may have treated a symptom instead of the cause.
- **Remove the condition instead of compensating for it.** If the evidence shows that one part always lands on the losing side, change what puts it there. A mechanism that cleans up after it leaves the cause in place and adds work on every run.

Do not start the next fix before the premise is written down and tested.
