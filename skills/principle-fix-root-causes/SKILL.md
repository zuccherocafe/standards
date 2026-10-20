---
name: principle-fix-root-causes
description: "Apply when debugging. Trace each symptom back to the first place the system went wrong and fix it there. A guard is a fix only when the state it handles is legitimate."
---

# Fix root causes

When debugging, fix the fault, not the place where it surfaced ([Weigh each fix, then fix the cause](../../rules/general.md#design)). The symptom is where a wrong state became visible. The root cause is the first place the system did something it should not do, or failed to handle a legitimate state. A symptom fix hides the evidence, leaves the fault free to surface elsewhere, and adds an unnamed workaround to the code.

Follow these steps when tracing a failure:

- Reproduce the failure first.
- Ask "why" at each step until the answer is a wrong decision in the code rather than a consequence of an earlier one.
- When stuck, add instrumentation. Read the actual error and state instead of guessing.
- Ask whether the state that triggered the symptom is legitimate. If the system may legitimately hold that state, the code that broke on it is the cause, so handle the state there. If the state should never exist, a guard where it broke hides the bug instead of fixing what produced the state. Guards, retries, swallowed errors, clamped values, and discarded state usually hide bugs. When a dependency mishandles an input, a refusal of that input at our boundary is a fix at the cause ([Weigh each fix, then fix the cause](../../rules/general.md#design)).
- When something fails only after a restart, check persisted state first: configuration files, caches, lock files, and serialised state.
- Search the codebase for the same fault and fix every site that a real input can reach.
- If a workaround needs a paragraph-long comment to justify it, fix the code instead of writing the comment.
- If the deciding fact was hard to observe, propose a cheap, always-on diagnostic that would show it immediately next time.
