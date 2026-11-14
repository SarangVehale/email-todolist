# Test plan: the release or change it covers

> **Status:** draft | current · **Audience:** release manager, maintainers and testers · **Last reviewed:** YYYY-MM-DD

<!-- docs/testing/test-plan.md is the plan for the next release: update it in place (clear the results and the sign-off). For a
     risky change that is not a release, copy this file to docs/testing/test-plan-<name>.md and add it to the documentation map
     in docs/README.md. A plan is a checklist someone can sign: every gate has a command or a
     case and a pass criterion that can be answered yes or no. Say what was not run; never leave a result looking green by
     leaving it out. The strategy (what is tested where) lives in docs/testing/strategy.md; this file only applies it. -->

One or two sentences: which version or change this plan covers, and the decision it supports (ship or do not ship).

## Scope

| In scope | Out of scope (and why) |
| --- | --- |
| What is tested in this release | What is not, with the reason (for example: not changed since the last release) |

## Environments

Name every system the tests run on and say honestly which have not been run.

| Environment | What runs there | Status |
| --- | --- | --- |
| Operating system, version | Engine tests, window tests, installer | verified, or not yet run |
| Browser and Node version | Window tests | verified, or not yet run |
| A clean machine | Install, first run, uninstall | verified, or not yet run |
| A real mailbox | Manual cases | verified, or not yet run |

## Entry criteria

What must be true before testing starts (for example: the version is bumped everywhere, the changelog has a dated entry, the
working tree is clean, the previous run's findings are fixed).

## Exit criteria

What must be true before the sign-off (for example: every gate below passes, every manual case in the required sections has a
recorded result, no open defect that loses data or sends mail, known problems written in the release notes).

## Gates

One row for each automated or scripted gate. Run them in order; stop at the first red one.

| # | Gate | Command | Pass criterion | Result |
| --- | --- | --- | --- | --- |
| 1 | Name of the gate | `the exact command` | What you must see | |

## Manual cases

One row for each manual case that must be run, taken from [the manual test plan](../TESTCASES.manual.md). Record pass, fail or
not run, the date, and for a failure the text of the Log tab (never a password).

| Case | What it checks | Result | Date | Notes |
| --- | --- | --- | --- | --- |
| MT-nn | | | | |

## Risks and what is not covered

Anything the plan cannot show, so a reader does not take a green table for more than it proves.

## Sign-off

Leave this table empty in the template and the plan. It is filled in by the people who read the results, on the day they do.

| Role | Name | Date | Decision | Notes |
| --- | --- | --- | --- | --- |
| Maintainer | | | | |
| Release manager | | | | |
| Owner | | | | |
