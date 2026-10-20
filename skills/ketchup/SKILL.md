---
name: ketchup
description: "Catch the user up on what changed in this chat since their last message, in a few plain sentences and small visuals. Give each action item just enough context to make the call. Use for /ketchup."
disable-model-invocation: true
---

# Ketchup

Catch the user up on what changed in this chat since the last message they wrote before `/ketchup`, and help them understand each change, not only where it stands. Notifications and subagent results are not their messages. Assume the user remembers their last message and nothing after it.

Follow the [technical-writing](../technical-writing/SKILL.md) and [unslop](../unslop/SKILL.md) skills. Write in a plain voice: plain words, short sentences, no jargon, like one person catching up another. Say what changed for the user, not how you did it, such as "the tests pass" instead of "CI is green". Leave out hashes, file paths, commands, build numbers, and test counts unless the user needs one to act. Call each pull request, branch, or agent by a name the user knows, not by a label you invented. If a fact needs context to make sense, give the context or leave the fact out.

## Gather what changed

Keep this turn read-only. Make visuals only from existing data, and do not change code, start a program or job, or post anything. Collect your own work since the user's last message, reports from subagents, and messages from anyone else in this chat. Check the current state of each pull request, check run, or agent this chat touched. Verify every number you plan to show against its source, and note any correction.

An action item is a decision or step that only the user can take, such as a product choice, an approval, an irreversible action, or access you lack. Include any action item still waiting on the user, even one raised before their last message. Work you can still do yourself is not an action item, and never invent a decision to fill the reply. Omit any step that the user did not ask for and the work does not need, unless it is irreversible and already waiting on the user. For such a step, give one action line with no visual, and skip what happens if the user does not answer.

## Write the reply

Keep the whole reply well under 150 words, not counting visuals. Draw a picture whenever it explains something better than sentences can, such as how a design or system works, a pull request stack or dependency chain with each branch's state, a before-and-after comparison, or options side by side. Never draw a status list or disconnected boxes. Never state a fact twice across text and visuals. Leave out work that changed nothing for the user. When space is tight, cut from what happened before cutting from the action items.

Open with one sentence that says how many things need the user, or that nothing does. If nothing happened and nothing waits on the user, say so and stop.

Then show what happened, pictures first. Embed inline the screenshots and videos the work produced, then any diagram that explains a change. Then sum up in one short bullet per thing the user asked for: what came of it, and whether it is done, still running, failed, or not checked. Write each line the way you would say it out loud: "Search was slow from a missing index", not "Search review: done" or "Slow search: missing index". Name any bug found and fixed on the way. Each line must stay true when read alone. Never mark something done that still waits on the user, and leave out anything the action items say. Under the summary, give at most three short lines that change what the user knows or must do. Then name in one line any work left that you can do yourself.

Then list the action items as plain bullets, never as a diagram or box. Give each two lines, or one for an unasked irreversible step: the decision with your recommendation and why, then what happens if the user does not answer. Ask the call the user owns, such as merge, approve, or pick. A step you would take first goes in your recommendation. So does anything that must happen first, and anything that cannot be undone. Write "unknown" for a part you do not know. Link to the detail.

Show any multi-option choice after the list. If the screenshots above already show the options, write each option's gain and cost under its screenshot instead of drawing a new visual. Otherwise, compare the options side by side, including the current state when there is one. Show every option the work weighed, with its benefit and cost to the user, and keep that comparison out of the rest of the text.

## Draw clear visuals

A visual is a screenshot, a video, a Mermaid diagram, a chart, or a generated image. A Markdown table is not a visual, even with before-and-after columns.

Keep each visual small enough to read at a glance, with at most six boxes and at most eight words per box. Write labels as plain words a person would say, not tags such as "Tests: pass". Put any fact that needs a full sentence in the text, or omit it.

Embed screenshots and videos inline when a change is visible on screen, rather than only linking them. If a visible change has no screenshot yet, start its summary line with "No screenshot yet," and describe what changed on screen. Draw diagrams and charts from facts you have: a Mermaid diagram for how things connect and a chart for numbers. Never invent a number, and never draw or generate a picture of a screen you did not capture. Link only artefacts that exist.
