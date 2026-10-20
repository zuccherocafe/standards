---
name: principle-encode-lessons-in-structure
description: "Apply when you catch yourself writing the same instruction a second time, or notice a recurring correction. Encode the rule as a lint, metadata flag, runtime check, or script instead of adding more prose."
---

# Encode lessons in structure

When you write the same instruction a second time or make the same correction twice, encode the rule as a lint, metadata flag, runtime check, or script instead of adding more prose. When a structural fix works, use only the structural fix and delete the prose instruction.

If the lesson requires human judgement that a tool cannot check, make the instruction prominent and add a concrete example of the failure.
