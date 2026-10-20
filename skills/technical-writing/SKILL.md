---
name: technical-writing
description: "Layered technical-writing standard: Diátaxis structure, Google developer style sentences, STE instruction rules, Global English syntax. Use for /technical-writing or when writing or reviewing docs, RFCs, readmes, PR descriptions, or commit messages."
disable-model-invocation: true
---

# Technical writing

Write so a tired engineer understands on the first read. Apply all four layers.

Three rules sit above the layers:

- **Cut every word that does no work.** If a sentence keeps its meaning without a word, delete the word, as in [unslop](../unslop/SKILL.md#filler) rule 23.
- **Use the short, everyday word.** Use a longer word only when it is more precise, as in [unslop](../unslop/SKILL.md#plain-speech) rule 31.
- **When a rule makes a sentence worse, fix the sentence another way or leave it alone.** A sentence that follows every rule and sounds like a machine wrote it has failed.

The codebase is the word list. Write the real symbol, file, flag, or command name, not a synonym or a description of it.

Do not invent jargon. Use the plain words a developer would say out loud, as in [unslop](../unslop/SKILL.md#jargon) rule 26. A named pattern is fine when the document defines it the first time it appears. When you find a new abstract metaphor and its plain replacement, propose adding them to `unslop` rule 26 with a diff in your reply. Do not edit that skill directly.

## Vary the rhythm

A document can obey every layer and still sound machine-written if every sentence is clipped short, takes no view, and states nothing specific.

- Mix sentence lengths on purpose. Short sentences land a point. Longer ones carry a fact with its condition or consequence.
- One thought per sentence does not mean one length per sentence. Split the sentence that carries two thoughts. Keep the long sentence that carries one.
- Have a view where the mode allows it. Explanation weighs trade-offs, so say what you make of them instead of listing pros and cons. Reference stays dry.
- Be specific over sterile. Not "schema changes can cause issues" but "a column rename fails the build".

## Pick the mode first (Diátaxis)

Give each document one mode. Answer two questions to choose it: does the document guide action or build understanding, and does it serve learning or work?

- Action + learning: **tutorial**.
- Action + work: **how-to**.
- Understanding + work: **reference**.
- Understanding + learning: **explanation**.

Use the compass on a whole document or on one sentence.

**Tutorial: learning by doing.** The learner's success is your job, not theirs. Open by saying what the learner will build, not what they will "learn". Every step produces a visible result, early and often. Tell them what they should see: the expected output, the prompt change, the log line. Cut explanation to one clause and a link. Stay concrete. Write as "we", in commands: "First, do x. Now, do y."

**How-to: steps to a goal.** Solve a problem a person has, not an operation the machine can perform. Assume competence and skip teaching. Include only actions, and link to background or reference material instead of digressing. Allow branches and judgement, as in "If you want x, do y." Name the guide by the task, such as "How to calibrate the radar array", not "Radar array calibration".

**Reference: facts for lookup.** Describe only. Give no instructions, persuasion, or opinion. Be dry, complete, and exact. State facts, options, limits, and errors without hedging. Mirror the structure of the thing described so a reader can move between code and documentation easily. Put material where readers expect it, and generate it from code where possible.

**Explanation: understanding and why.** Cover one bounded topic so a reader can understand it away from the product. Write each title so "About..." fits in front of it. Answer a real "why" question with design decisions, history, constraints, and alternatives. State opinions here and nowhere else.

Never mix modes. Keep reference tables out of tutorials, tutorial steps out of reference, and design arguments out of how-to guides. Split them into separate documents and link them.

## Write sentences to the reader (Google developer style)

- Address the reader as "you" in the present tense. Use "will" only for events that genuinely happen later.
- Say who does what, as in [unslop](../unslop/SKILL.md#plain-speech) rule 29.
- Write instructions and procedures as direct commands, such as "Click **Submit**.", never as narration or in the passive. State facts plainly, and never write "should be done".
- Put the condition or warning before the instruction it guards, as in "To delete the document, click **Delete**."
- Put the common case first and exceptions after it.
- Write like a knowledgeable colleague. Avoid buzzwords, figurative language, "please" in instructions, and words such as "simply", "easy", or "quickly" in a procedure. If it were simple, the reader would not be here.
- Do not pre-announce future features, and do not start consecutive sentences with the same phrase.
- Write link text that names the destination, such as the page title or a short description, and never write "click here". Prefer a sentence of context on the page over a link off it.
- Write headings in sentence case, as in [unslop](../unslop/SKILL.md#style) rule 17, that state the point, not only the topic, such as "Pick the mode first" instead of "Modes". Use a bare verb phrase for a task heading, such as "Create an instance", and a noun phrase for a concept heading. Use one h1 per page and do not skip heading levels.
- Use numbered lists for sequences and bulleted lists for everything else. Introduce every list with a complete sentence, and keep list items parallel.
- Code goes in code font. UI elements go in bold. Use serial commas. Drop "etc." and say up front that a list is partial.

## Give one instruction or thought per sentence (STE)

- Give one instruction per sentence in procedures, and one thought per sentence everywhere else, as in [unslop](../unslop/SKILL.md#plain-speech) rule 28.
- Split instructions longer than about 20 words and other sentences longer than about 25 words.
- Keep articles such as "the" and "a". "Remove backup file" reads two ways, whereas "Remove the backup file" reads one.
- Give each word one meaning and one job, then keep it. If "check" means inspect, do not also use it to mean restrain.
- Pick one verb per action and keep it, such as "start" everywhere rather than "start" in one place and "initiate" in another.
- Avoid "-ing" words where you can.

## Leave no sentence open to two readings (Global English)

- Keep words like "only" and "not" next to the word they change: "only fails on growth" and "fails only on growth" say different things.
- Break up long noun strings: "the proto import budget check script" becomes "the script that checks the proto-import budget".
- Make every "it", "they", and "this" point at one obvious thing. Repeat the noun when in doubt. Never use "this" or "which" to point at a whole clause.
- Don't drop verbs: "Phase 1 moves the converters and Phase 2 the runtime" leaves Phase 2 without one. Give it one.
- Keep the small words that show structure, such as "that" in "Ensure that the switch is off". Never trade clarity for word count.
- Repeat the article in a series when it prevents a misread: "the client and the host", not "the client and host", when they are two things.
- Say which parts "and" or "or" joins when a sentence can group two ways. "Both...and", "either...or", and "if...then" are free disambiguators.
- Use periods, not semicolons. Replace an em dash with a new sentence.
- Make text in parentheses a full grammatical unit or its own sentence. Never form plurals with "(s)".
- Do not use slashes. Write "a, b, or both" instead of "a/b" or "and/or".
- Call each thing by one name everywhere, as in [unslop](../unslop/SKILL.md#language) rule 11. A document that calls one check "the gate", "the ratchet", and "the budget check" teaches three things.
- Do not rewrite unchanged passages only to reword them.
- Skip idioms, colloquialisms, Latin abbreviations, and metaphors.

## Voice and repo specifics

- Apply the [unslop](../unslop/SKILL.md) skill to every document you write or edit. That skill holds the catalogue of AI vocabulary, filler, hedging, and formatting tells.
- Apply every layer except Diátaxis to pull request descriptions and commit messages. Keep a pull request body short enough for a reviewer to read in under a minute, and link logs, commit lists, or metric tables instead of pasting them.
- Follow your product's copy guidelines for product UI strings.
- Indent code snippets with tabs. Write real paths. Make every count or directory-tree claim true at the commit that lands it, and include the command that regenerates it.

## Worked example

Before:

> Configuration of the proto import ratchet budget script parameters is performed via budget.json. Note that it's important to remember that running with --write, which updates the committed budget to reflect the current count, should only be done when lowering it. If exceeded, CI fails.

After:

> `budget.mjs` reads the committed budget from `budget.json` and counts the files that import protos. If the count exceeds the budget, CI fails. Run `budget.mjs --write` only to lower the budget.
