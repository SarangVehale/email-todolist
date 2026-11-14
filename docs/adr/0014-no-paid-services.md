# ADR-0014: No paid services of any kind

> **Status:** accepted · **Date:** 2026-11-13 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The owner has no money for paid services and set this as a constraint that does not change. It is written down in every design
from [Phase 2](../designs/phase2.md) on ("No paid service"), most sharply in [Phase 3](../designs/phase3.md): "No paid service
of any kind", and "The owner has no money for paid services, so a shared Google client ID is out". [Phase 10](../designs/phase10.md)
rules out a hosted wiki for the same reason. The [README](../../README.md) promises that rules-only ranking, the default, sends
nothing out.

## Decision

We build and run the app with no paid service: no server of ours, no hosted relay, no shared API key, no paid certificate or
review. Anything that costs money is the user's own optional choice (Anthropic with their own key) and is never turned on by the
app or by a restored backup.

## Consequences

* The app works fully for free: rules-only ranking, IMAP with app passwords, the phone hub on your own network, ntfy's free
  server for alerts, GitHub for releases and CI.
* Google sign-in is deferred: a public app with the restricted Gmail scope needs a paid yearly assessment ([Phase 3](../designs/phase3.md)).
* Releases are unsigned until signing certificates are added, and macOS notarization needs a paid Apple account
  ([RELEASING.md](../../RELEASING.md)). Windows and macOS warn on install.
* The phone hub has no TLS certificate ([ADR-0009](0009-phone-hub-plain-http-with-token.md)), and the hub cannot be reached
  away from your own network without a VPN you set up.
* A restore never turns on the paid cloud ranking ([Phase 7](../designs/phase7.md)).
* Revisit only if the owner sets a budget.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| A shared Google client ID for sign-in | Needs a paid yearly security assessment for the restricted scope. |
| A hosted relay for the phone | A server to pay for and run, and mail would leave the computer. |
| A model key provided by the project | A running cost, and mail text would go to a service the user did not choose. |

## References

* [Phase 3 design](../designs/phase3.md), [README.md](../../README.md), [PRIVACY.md](../../PRIVACY.md)
