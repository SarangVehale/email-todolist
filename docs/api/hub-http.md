# Phone hub HTTP interface

> **Status:** current · **Audience:** developers who change the phone hub or the window's phone mode · **Last reviewed:** 2026-11-14

The phone hub is a small HTTP/1.1 server inside the engine ([`engine/src/hub.rs`](../../engine/src/hub.rs)) that serves the
window's files and one command endpoint to a phone or tablet on your own network. This page lists exactly what it accepts and
answers; the same interface is in [openapi.yaml](openapi.yaml).

## At a glance

| Item | Value |
| --- | --- |
| Turned on by | `hub.enabled` in the settings ("Phone hub" in Settings, or the setup wizard) |
| Listens on | every interface, `0.0.0.0:<hub.port>` (default 8787), plain HTTP, no TLS |
| Answers | only peers on loopback, private (10/8, 172.16/12, 192.168/16), link-local, 100.64/10 (Tailscale) and IPv6 local (`fc00::/7`, `fe80::/10`) addresses; any other peer's connection is closed without an answer |
| Routes | `GET` on the 14 file paths below; `POST /api/rpc` |
| Access code | 32 letters and digits, sent as `Authorization: Bearer <code>`, compared in constant time |
| Lock-out | 10 wrong codes from one address within 60 seconds: that address gets `429` until 60 seconds after its first failure |
| Limits | request head 16 KB, body 64 KB, a 5-second timeout on each read and each write (not on the whole request), 32 connections at once, one request per connection |
| Commands | `state` (reduced) and the 12 in `commands::PUBLIC`; see [the overview](README.md#what-is-desktop-only) |

## How a request is handled

This shows the checks in the order the code makes them.

```mermaid
flowchart TD
    A["Connection accepted"] --> B{"Peer on a local network?"}
    B -->|no| X1["Closed, no answer"]
    B -->|yes| C{"Fewer than 32 open?"}
    C -->|no| X1
    C -->|yes| D["Read head and body (5 s limit on each read)"]
    D -->|"too big, malformed, or a read that stalls"| X2["400, 408, 413 or 431"]
    D --> E{"Method and path"}
    E -->|"GET, known file"| F["200 with the file"]
    E -->|"GET, other path"| X3["404"]
    E -->|"POST /api/rpc"| G{"Address locked out?"}
    E -->|"other method on /api/rpc"| X4["405"]
    E -->|"anything else"| X3
    G -->|yes| X5["429"]
    G -->|no| H{"Bearer code matches?"}
    H -->|no| X6["401, failure counted"]
    H -->|yes| I["Core::rpc_public"]
    I --> J["200 ok/data, or 400 ok false/error"]
```

## Routes

### `GET` a file

The files are built into the engine at compile time (`FILES` in `hub.rs`); the hub never reads the file system. The query
string is ignored, so `/?token=...` serves `/`. Any other path, including `GET /api/rpc`, is `404`.

| Path | Content-Type |
| --- | --- |
| `/`, `/index.html` | `text/html; charset=utf-8` |
| `/style.css` | `text/css; charset=utf-8` |
| `/main.js`, `/drafts.js`, `/setup.js`, `/boot.js` | `text/javascript; charset=utf-8` |
| `/manifest.json` | `application/manifest+json` |
| `/icon.svg` | `image/svg+xml` |
| `/fonts/bodoni-moda-latin-wght-normal.woff2`, `/fonts/bodoni-moda-latin-wght-italic.woff2`, `/fonts/ibm-plex-mono-latin-400-normal.woff2`, `/fonts/ibm-plex-mono-latin-500-normal.woff2`, `/fonts/space-grotesk-latin-wght-normal.woff2` | `font/woff2` |

The demo backend (`mock.js`) and `demo.html` are not served. A file is served without an access code; the code guards only
the commands.

### `POST /api/rpc`

```http
POST /api/rpc HTTP/1.1
Host: 192.168.1.20:8787
Authorization: Bearer AbCdEfGhIjKlMnOpQrStUvWxYz012345
Content-Type: application/json
Content-Length: 55

{"cmd": "set_star", "args": {"uid": 812, "star": true}}
```

* **Body:** JSON `{"cmd": string, "args": object}`. `Content-Type` is not checked. A body that is not JSON is read as an empty
  command and answered `400` with `not available from the hub`. Chunked transfer is not supported: the body is read by
  `Content-Length` only.
* **Success:** `200` with `{"ok": true, "data": <the command's answer>}` (see [the command reference](rpc.md)).
* **Command error:** `400` with `{"ok": false, "error": "<message>"}`. A command outside the allow-list, including a name
  the engine does not know, gets `not available from the hub`.

## The access code

* Made by `core::new_token` from a cryptographic random generator: 32 characters from `A-Z`, `a-z`, `0-9`. Stored in
  `settings.json` as `hub.token`.
* A new code is made when the hub is switched on without one; switching the hub off clears it; "Rotate access code"
  (`rotate_hub_token`) replaces it; "Sign out and reset" clears it and turns the hub off.
* The desktop's Settings shows it inside a QR code of `http://<this computer's address>:<port>/?token=<code>` (`hub_info`).
  The window opened from that link saves the code in the browser's `localStorage` and removes it from the address bar, so
  it is not left in history or a `Referer`.
* Each command request sends it as `Authorization: Bearer <code>`. The scheme name is case-insensitive; any other scheme,
  or none, counts as a wrong code. If the hub has no code at all, every request is `401`.

## Lock-out

Failures are counted per peer address in memory (they reset when the hub restarts, which every settings save does).

* A wrong or missing code adds one failure. The count starts at the first failure and is reset when 60 seconds have passed
  since then.
* Once an address has 10 failures inside that window, every `POST /api/rpc` from it, even with the right code, gets `429`
  until the window ends. Files are still served.
* A right code does not reset the count.
* The table holds at most about 4,096 addresses; past that, expired entries are dropped.

## Response headers

Every response, file or JSON, carries:

```http
Content-Type: <per route; application/json for /api/rpc and errors>
Content-Length: <n>
Connection: close
Cache-Control: no-store
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Referrer-Policy: no-referrer
Cross-Origin-Resource-Policy: same-origin
Cross-Origin-Opener-Policy: same-origin
Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=()
Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
```

There are no CORS headers: only pages served by the hub itself can call it from a browser.

## Status codes

| Code | When | Body |
| --- | --- | --- |
| `200` | A known file, or a command that succeeded | the file, or `{"ok": true, "data": ...}` |
| `400` | Request line malformed (no method, target not starting with `/`, not `HTTP/1.x`); a header line without `:`; `Content-Length` not a number; the connection closed before the whole body arrived; or the command failed or is not allowed | `{"ok": false}` for a bad request; `{"ok": false, "error": "..."}` for a command |
| `401` | `POST /api/rpc` without the right code (counted towards the lock-out) | `{"ok": false, "error": "unauthorized"}` |
| `404` | `GET` of any path not in the file table; any method other than `GET` or `POST` on a path other than `/api/rpc`; `POST` on a path other than `/api/rpc` | `{"ok": false, "error": "not found"}` |
| `405` | A method other than `GET` or `POST` on `/api/rpc` (for example `PUT`) | `{"ok": false}` |
| `408` | A read of the head waited 5 seconds for more bytes, or a read of the body failed or timed out. The 5 seconds is a limit on each read and each write, not on the whole request: a client that sends a byte every few seconds can hold a connection until the head or body size limit | `{"ok": false}` |
| `413` | `Content-Length` over 64 KB (65,536 bytes) | `{"ok": false}` |
| `429` | The address is locked out | `{"ok": false, "error": "too many attempts"}` |
| `431` | The head grew past 16 KB without its blank line | `{"ok": false}` |

No other code is sent. A peer that is not on a local network, a 33rd connection, and a client that closes before sending a
whole head get no answer at all.

## Related

* [The command reference](rpc.md) for what each allowed command does.
* [The phone and tablet guide](../guides/phone-and-tablet.md) and the
  [phone hub runbook](../runbooks/phone-hub-unreachable.md).
* [SECURITY.md](../../SECURITY.md) for the review findings that shaped these limits.
