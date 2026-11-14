# User story: what the person wants, in a few words

> **Status:** current · **Audience:** product owner, maintainers and testers · **Last reviewed:** YYYY-MM-DD

<!-- Copy the story block below into docs/product/user-stories.md under its epic, and give it the next id (US-NN). A story is
     a need, not a task: it says who wants what and why, how we will know it works, and which test proves it. A story with no
     "Verified by" line is not finished. -->

## US-NN: Short name of the story

> **Epic:** the group it belongs to · **Priority:** must | should | could · **Status:** built | partly | not built · **Requirements:** FR-NN

**As a** kind of person (a persona from the [product requirements](../product/prd.md)),
**I want** what they want to do,
**so that** the result they care about.

**Acceptance criteria**

Write each one so that a tester can answer yes or no. Prefer the form "Given a situation, when the person acts, then this
happens" and include at least one refusal or failure case.

* Given ..., when ..., then ...
* Given ..., when ..., then ...
* What must never happen (for example: nothing is sent, no password is written to a file).

**Out of scope**

What this story does not promise, so nobody reads more into it.

**Verified by**

Name real tests, and check each name exists before you write it:

* Engine tests: `test_name` (`engine/tests/file.rs`).
* Window tests: the exact title from `app/test/*.test.mjs`.
* Manual cases: `MT-nn` from [the manual test plan](../TESTCASES.manual.md).

## Checks before a story is accepted

| Check | Why |
| --- | --- |
| The person, the want and the result are all stated | A story without a result cannot be judged |
| Every criterion can be answered yes or no | A vague criterion cannot fail |
| Each criterion has a test or a manual case that fails when it is broken | Otherwise the story is a hope |
| It uses the glossary words: task, mailbox, conversation, window, engine, hub, app password | One name for each thing |
