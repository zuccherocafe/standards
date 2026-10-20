---
name: unslop
description: "Apply to every piece of prose you write or edit: documentation, comments, commit messages, reviews, reports, and replies. Cut AI tells from it."
---

# Unslop

Edit text to remove AI patterns. Scan for the patterns below, then rewrite. Preserve meaning and match the intended tone.

## Patterns to detect and fix

Rule numbers are stable identifiers that other skills cite. Removing a rule leaves a gap in the numbering.

### Content

3. **Superficial -ing phrases.** Delete trailing participles such as "highlighting...", "ensuring...", "reflecting...", "showcasing...", and "fostering...", or expand with real sources.
5. **Vague attributions.** Delete unattributed claims such as "Experts believe", "Industry reports suggest", and "Some critics argue", or name the source.

### Language

7. **AI vocabulary.** Replace words such as "additionally", "crucial", "delve", "enduring", "enhance", "fostering", "garner", "interplay", "intricate", abstract "landscape", "pivotal", "showcase", abstract "tapestry", "testament", "underscore", and "vibrant" with plain words.
8. **Fancy ways to say "is".** Replace "serves as", "stands as", "boasts", and "features" with "is" or "has".
9. **"Not just X, but Y."** State the point directly instead.
10. **Rule of three.** Do not force ideas into groups of three. Use the natural number of items.
11. **Synonym cycling.** Do not cycle through synonyms such as "protagonist", "main character", "central figure", and "hero" in one paragraph. Pick one term and repeat it.
12. **False ranges.** Do not write "from X to Y" when X and Y are not on a real scale. List the items directly.

### Style

13. **Em dash overuse.** Avoid em dashes entirely. Use periods or commas only, with no parentheses, en dashes, or hyphen-as-dash substitutes. If a thought needs separation, end the sentence or use a comma.
14. **Colon overuse.** Use colons before a list or example, not as mid-sentence connectors. "If you're coming from traditional automation: instead of registering event handlers, you describe conditions" adds nothing with the colon. Rewrite the sentence so the point stands on its own, as in "Describing when the scheduler should fire works best as plain English."
15. **Boldface overuse.** Do not bold every proper noun or acronym.
16. **Inline-header lists.** Do not start list items with a bold label and colon that restates the line, as in "**Performance:** Performance improved...". Convert those items to prose. A bold lead-in that ends in a period, names the item, and precedes genuinely new detail is fine, as in "**Schema in TypeScript.** Tables live in one file."
17. **Title case headings.** Use sentence case.
18. **Decorative emojis.** Remove emojis from headings and list items.
19. **Curly quotes.** Use straight quotes.

### Communication artefacts

20. **Chatbot phrases.** "I hope this helps!", "Let me know if...", "Of course!", "Certainly!", "Found the smoking gun!" Remove.
22. **Sycophantic tone.** "Great question! You're absolutely right!" Respond directly.

### Filler

23. **Filler phrases.** "In order to" becomes "To". "Due to the fact that" becomes "Because". Delete "It is important to note that".
24. **Excessive hedging.** "could potentially possibly be argued that it might" becomes "may".
25. **Generic conclusions.** "The future looks bright." State specific plans or facts.

### Jargon

26. **Abstract metaphor nouns.** Replace metaphor nouns with plain concrete words: "substrate" becomes "base", "wedge" or "wedge in" becomes "add", "vector" becomes "way" or "method", "gold-plating" becomes "more than the job needs", "ratchet" becomes the mechanism's real name or "a limit that only tightens", "evacuate" for moving code becomes "move out", and "endgame" becomes "the last phase". Also avoid "locus", "vantage", "nexus", "primitive" as a noun, "harness" or "scaffolding" as a metaphor, "surface" as in "API surface", "bedrock", "modality", "paradigm", "north star", and "flywheel".

### Plain speech

27. **Say what it does, not how it feels.** Phrases such as "the database stays close at hand", "SQL you can read", and "types that follow your schema" name a feeling. Name the mechanism or a number instead: "`.toSQL()` returns the exact string sent to the database" or "a column rename fails the build". Ask what the sentence tells the reader to do or know, then write that. If you cannot restate it as a concrete instruction, fact, or number, or if the sentence could appear unchanged in another project's documentation, cut it.
28. **Shorten or split dense sentences.** If the reader has to backtrack to parse a sentence, split it in two or drop clauses. Keep one idea per sentence.
29. **Active voice.** Prefer active voice. Replace passive forms such as "is", "are", "was", or "were" followed by a past participle by naming the actor: "queries are validated" becomes "the compiler validates queries", and "the file is parsed by the loader" becomes "the loader parses the file". Use the passive only when the actor is unknown or genuinely beside the point.
30. **Cut adverbs, or use a stronger verb.** "runs quickly" becomes "is fast" or the measured time, and "significantly improves" becomes the measured delta. An adverb that props up a weak verb means the verb is wrong.
31. **Prefer the plain word.** "utilize" and "leverage" become "use", "facilitate" becomes "help", "numerous" becomes "many", and "in the event that" becomes "if". The fancier synonym is rarely clearer.
32. **Mannered prose.** Avoid metaphor or flourish where a literal phrase exists, including aphorisms such as "wire it or delete it", rhetorical fragments for effect, personified code such as "the plan holds it", figurative verbs such as "rides along" or "stands on", and stock framing phrases. "A dial worth turning" becomes "a parameter worth varying". Say what you mean. Rule 26 covers metaphor nouns.
33. **Over-compression.** Dropped articles, verbless fragments, symbol-speak, and abbreviations that make the reader decode instead of read. "Parser rejects bad date → exit 2, no write" becomes "The parser rejects a bad date, exits with code 2, and writes nothing." Write whole sentences with their articles and verbs, and spell out arrows and abbreviations.
