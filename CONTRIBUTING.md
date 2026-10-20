# Contributing to zucchero✿café

zucchero✿café holds its code to very high standards, the ones in this repository. If you can't or won't hold yourself to these standards, don't open a pull request.

We encourage using AI when the code it produces meets these standards. We use it to raise the quality of our code, not only to write it faster. Most AI-written pull requests do not meet them and would fail [the review's first questions](https://github.com/zuccherocafe/standards/blob/main/rules/review.md#first).

If your contribution is prompting an AI and slinging its shit over the fence to us, the prompt itself is the only thing of value to us. Put it in an issue and let us handle it. We can type a prompt as well as you can, and we know the codebase well enough to tell the AI when it is wrong.

## Write a detailed issue

We can only act on an issue as well as it describes the problem. Write it so someone who has never seen your setup can reproduce the problem without asking you anything.

## Before you open a pull request

1. Read [`general.md`](https://github.com/zuccherocafe/standards/blob/main/rules/general.md) and the [standard for your language](https://github.com/zuccherocafe/standards/tree/main/rules/languages).
2. Start your AI inside the project's development shell with `nix develop`, so it gets the pinned tools and the standards' skills.
3. Have your AI run the project's full checks at the tip of your branch.
4. Review the change against [`review.md`](https://github.com/zuccherocafe/standards/blob/main/rules/review.md) yourself, or have an AI run the `change-review` skill. Fix every must-fix finding, and fix each should-fix finding unless it costs more than a few lines or changes the design.

Include no `Co-Authored-By` line or any other attribution in your commits. You are responsible for every line you send. A co-author line hands part of that responsibility to an AI and only adds an advert for the vendor to the commit log.

## Write the description yourself

Requesting human attention requires human effort.

Write the pull request description yourself, in your own words. The description is the one part of the pull request that shows you understand the change. If the reviewer thinks an AI wrote it, they will close the pull request without reading the code.

Don't describe the diff. Explain how you understand the part of the system you changed: what the problem is, why it happens, and why your change is the right fix. If you considered another approach and dropped it, say why.

Write it as carefully as the change deserves. A typo fix would only need a sentence, but a change to how part of the system works needs more considered attention.

## Review

A maintainer may ask you about your change. Answer in your own words. If you paste an AI's answer to a question about your own change, we will close the pull request.

Fold each fix into the commit it belongs to and force-push the branch.

## Licence

Your contribution is published under the project's licence.
