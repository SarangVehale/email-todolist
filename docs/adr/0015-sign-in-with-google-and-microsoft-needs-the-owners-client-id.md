# ADR-0015: Sign in with Google and Microsoft is built, and off until the owner registers the app

> **Status:** accepted · **Date:** 2026-11-15 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-15

## Context

The owner asked for "direct Google login", and for an app a grandmother can set up ([Phase 12](../designs/phase12.md)). An app password
works for every provider that offers one, but making one takes a visit to a page most people have never seen. Signing in with the account
is one click. Three facts shape it:

* Reading mail over IMAP with a Google account needs the scope `https://mail.google.com/`, which Google calls **restricted**. An app that
  has not been verified is limited to 100 users and shows "Google hasn't verified this app" (the person presses Advanced, then Continue). In a
  project whose publishing status is Testing, refresh tokens last seven days. Removing all of that needs Google's verification and a yearly
  security assessment that costs money. [ADR-0014](0014-no-paid-services.md) rules that out.
* Microsoft turned off password sign-in for IMAP on personal accounts (Outlook, Hotmail, Live), so for them there is no app-password way in at
  all. Its free route is an app registration (a public client with no secret) and the device-code flow.
* Both need a **client ID** registered by a person with an account at the provider. No one can do that on the owner's behalf, and a client ID
  shared by strangers is the thing Google's verification exists to police.

## Decision

We build both sign-ins in the engine ([`oauth.rs`](../../engine/src/oauth.rs)) and the window, and offer them only when the build has a
client ID for them (`GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `MICROSOFT_CLIENT_ID`, empty today). Until the owner fills them in
([the runbook](../runbooks/register-sign-in.md)) the window shows no button and nothing changes. The app password stays the default and the
answer for everyone else.

* **Google:** the loopback flow with PKCE. A one-shot listener on `127.0.0.1` receives the code; it keeps accepting until a request carries the
  right `state`.
* **Microsoft:** the device-code flow, against the `consumers` tenant (personal accounts). The window shows the short code.
* **Tokens:** the refresh token lives in the system keychain (`oauth-refresh-<n>`), the access token only in memory. Tokens travel in a POST
  body or an IMAP command, never in an address, and no text from a provider is kept: a failure is one of a fixed list of codes.
* **Where a token goes:** a Google sign-in is only ever for `imap.gmail.com` and a Microsoft one only for `outlook.office365.com`, checked
  whenever settings are saved. Only the sign-in commands can change which mailboxes sign in this way.
* **IMAP:** XOAUTH2. A refused token is dropped and one new one is tried once; a withdrawn sign-in is not asked about again for five minutes.

## Consequences

* The feature is dormant in every build that does not name a client ID, and is tested end to end against scripted token and mail servers
  ([`engine/tests/signin.rs`](../../engine/tests/signin.rs)). It has not been run against Google or Microsoft: the owner's first run with a
  registered client ID is the test that matters ([manual cases](../TESTCASES.manual.md)).
* With Google's project set to **In production, unverified**, sign-in works for up to 100 people who press through a warning. That is for
  family and friends. Anyone else uses an app password.
* A new engine command family (`oauth_start`, `oauth_poll`, `oauth_cancel`, `oauth_finish`), a new setting (`oauth`), a new keychain entry
  per mailbox, and one more host the app opens (`www.microsoft.com`, where the device code is typed).
* **What would make us revisit it:** the owner paying for Google's verification (the 100-user limit and the warning go away); Microsoft
  allowing work and school accounts without an administrator; Google changing the restricted scope's rules.

## Alternatives considered

* **One shared client ID for everyone, in the build.** Rejected: it hits the 100-user limit at once for a public app, and Google's rules
  for restricted scopes need the paid assessment.
* **Each person registers their own client ID.** Rejected for grandmothers; it is the opposite of the goal. It remains possible for a technical
  person who edits the constants.
* **The Gmail API instead of IMAP.** Rejected: a different scope with the same verification, and the whole engine reads IMAP.
* **A hash crate for the PKCE challenge.** Rejected for one use: SHA-256 is written out in `oauth.rs` and tested against the standard's values
  and RFC 7636's example.
