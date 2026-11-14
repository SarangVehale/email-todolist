# ADR-0006: Rules rank mail first; a model is optional

> **Status:** accepted · **Date:** 2026-11-12 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The app must cost nothing to run ([ADR-0014](0014-no-paid-services.md)) and keep mail on the computer by default. A model ranks
better, but a local one needs Ollama installed and a cloud one needs your own paid key and sends mail text out
([PRIVACY.md](../../PRIVACY.md)). Model calls fail: timeouts, a stopped server, a reply that is not JSON. The design records say
"the default (rules only) must stay useful with no AI" ([Phase 2](../designs/phase2.md)).

## Decision

We rank every message with `RulesTriager` by default. A local model (Ollama) or Anthropic with your own key implements the same
`Triager` trait, and `triage_with_fallback` uses the rules whenever the model returns an error. Drafts fall back to a quick-reply
template the same way. The mode is `ai.mode` in the settings: `rules`, `ollama` or `anthropic`.

## Consequences

* The app works offline, for free, with nothing leaving the computer, from the first start.
* A failing model never loses or delays mail; its output is validated and anything unusable falls back ([security model](../architecture/security-model.md)).
* The rules had to become good on their own: priority 5 for a due question from someone you write to, learning from your
  corrections, due dates found without AI ([Phase 2](../designs/phase2.md)). They never set "urgent".
* Your rules and sender choices apply on top of any ranker (`Store::add`).
* A backup never restores the cloud mode ([Phase 7](../designs/phase7.md)); choosing it is always your own act in Settings.
* Revisit if rules alone stop being useful enough to be the default.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| A model required | A cost or an extra install before the app does anything, and mail text leaving the computer by default. |
| A hosted ranking service of our own | A server and a running cost, and mail would leave the computer. |

## References

* `engine/src/triage.rs`, `engine/src/ai.rs` (`make_triager`), `engine/src/signals.rs`
* [README.md](../../README.md), "Your choice of brain"
