# ADR-0009: The phone hub is plain HTTP with an access code

> **Status:** accepted · **Date:** 2026-11-12 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The owner wanted the same list on a phone or tablet, with setup inside the app (a QR code in Settings), no server of ours and no
paid service. The data never leaves the computer except to the phone. The first version used an HTTP library that had no
timeouts, so one stalled client could freeze it ([SECURITY.md](../../SECURITY.md), finding 3).

## Decision

When you switch it on, the engine runs its own small HTTP/1.1 server (`engine/src/hub.rs`) on every interface at the hub port
(8787 by default). It answers only peers on local networks (`hub::local_peer`), serves the window's files from a compiled-in
list, and has one command endpoint, `POST /api/rpc`, that needs the 32-character access code as a bearer token and accepts only
`state` and the commands in `commands::PUBLIC`.

## Consequences

* No relay, no account and no certificate: it works on home Wi-Fi or over a private VPN such as Tailscale.
* Every limit is ours: 5-second I/O limits, 16 KB headers, 64 KB bodies, 32 connections, a lockout after 10 bad codes a minute,
  hardening headers and a CSP on every response ([security model](../architecture/security-model.md#untrusted-inputs-and-their-bounds)).
* The phone can never reach accounts, settings, secrets, AI drafting, backups or write to the mailbox, even with a stolen code.
* Plain HTTP means the code can be sniffed on a shared network. SECURITY.md lists this as a residual risk; rotating the code
  limits the exposure.
* Revisit if the hub has to work on untrusted networks, or if TLS can be offered without a paid certificate or a manual trust step on the phone.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| An off-the-shelf HTTP library | The one used first had no timeouts, and a stalled client froze the hub (finding 3). |
| A relay or cloud service | A server of ours and a running cost ([ADR-0014](0014-no-paid-services.md)), and mail would leave the computer. |
| HTTPS with a self-signed certificate | Every phone would have to trust it by hand, against "all setup inside the app". |

## References

* `engine/src/hub.rs`, `engine/src/core.rs` (`rpc_public`, `hub_info`, `new_token`), `engine/src/commands.rs` (`PUBLIC`)
* [Hub HTTP reference](../api/hub-http.md)
