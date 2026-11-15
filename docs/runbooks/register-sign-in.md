# Runbook: register the app so "Sign in with Google" and "Sign in with Microsoft" appear

> **Status:** current · **Audience:** the owner of the project, who builds the app · **Last reviewed:** 2026-11-15

Use this once, before a build that should offer the two buttons. Nothing in it can be done for you: each step needs your own Google or
Microsoft account. It costs nothing. Without it the app works exactly as before (app passwords), and the window shows no button.
[ADR-0015](../adr/0015-sign-in-with-google-and-microsoft-needs-the-owners-client-id.md) says why.

## When to use this

* You want people to sign in with their Google account in one click instead of making an app password.
* You want people with Outlook, Hotmail or Live addresses to be able to use the app at all.

## Before you start

* A Google account for the project (not a work account that an administrator can close), and a Microsoft account.
* The honest limits, so nobody is surprised later:
  * **Google:** the permission to read mail is *restricted*. An app that Google has not verified is limited to **100 users** and shows
    "Google hasn't verified this app" to each of them (they press **Advanced**, then **Go to Inbox Agent**). Google's verification needs a
    paid yearly security assessment, which the project does not buy. So **Sign in with Google is for you, family and friends**. Strangers use an
    app password.
  * **Microsoft:** this covers personal accounts (Outlook.com, Hotmail, Live). Work and school accounts (Microsoft 365) need an administrator's
    consent and are not covered.
* A client ID is not a password, and the Google "secret" for a desktop app is not secret either: Google says so. They are written into the
  source, so anyone who builds the app uses your registration. Keep the project private if you do not want others to.

## Steps

### Google

1. Open [console.cloud.google.com](https://console.cloud.google.com) and create a project named Inbox Agent.
2. **APIs and services**, **OAuth consent screen**: choose **External**. Fill in the app name (Inbox Agent), your address as the support
   address and as the developer contact. Under **Scopes**, add `https://mail.google.com/`.
3. On the same screen, set **Publishing status** to **In production**. Leave it unverified. **Do not leave it on Testing:** in Testing a
   sign-in stops working after seven days and each person has to be added by hand.
4. **Credentials**, **Create credentials**, **OAuth client ID**, application type **Desktop app**. Name it Inbox Agent. A "Web application"
   client does not work: it fails with `redirect_uri_mismatch`.
5. Copy the **client ID** and the **client secret** it shows.
6. Put them in [`engine/src/oauth.rs`](../../engine/src/oauth.rs): `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET`.

### Microsoft

1. Open [entra.microsoft.com](https://entra.microsoft.com), **App registrations**, **New registration**. Name it Inbox Agent. For
   **Supported account types** choose **Personal Microsoft accounts only**. Leave the redirect address empty.
2. **Authentication**, **Advanced settings**: turn **Allow public client flows** on (this is what lets the device code work with no secret).
3. **API permissions**, **Add a permission**, **APIs my organization uses**, search for **Office 365 Exchange Online**, **Delegated
   permissions**: add `IMAP.AccessAsUser.All`. Add `offline_access` from **Microsoft Graph** (it is what makes the sign-in last).
4. Copy the **Application (client) ID** from the overview page.
5. Put it in [`engine/src/oauth.rs`](../../engine/src/oauth.rs) as `MICROSOFT_CLIENT_ID`. There is no secret.

### Both

1. Run the [engine tests](../testing/strategy.md) (`cd engine && cargo test`) and build the app. The two buttons now appear for the provider
   whose ID you filled in.
2. Try each by hand, using the [manual cases](../TESTCASES.manual.md) MT-150 to MT-156, with an account of your own.

## Check that it worked

On a clean install, type a Gmail address. The next screen says **Connect your Gmail** and has **Sign in with Google**. Press it: your browser
opens Google's page; you sign in (and, until verification, press through the warning); the window moves to "You're connected." A Hotmail address
shows **Sign in with Microsoft** and a short code to type.

## If something goes wrong

| What you see | Why | What to do |
| --- | --- | --- |
| No button appears | The ID is empty, or the build is an old one | Check the constants, rebuild |
| "Google did not accept this app's sign-in settings…" (`invalid_client`) | A wrong ID or secret, or a Web client instead of a Desktop client | Make a Desktop client and copy both values again |
| Google says `redirect_uri_mismatch` or "Access blocked: this app's request is invalid" | The client is of the wrong type | Make a **Desktop app** client |
| "Mail access was not allowed…" | The person unticked the mail permission on Google's page | Sign in again and leave the box ticked |
| "Google did not give Inbox Agent lasting access…" | Google gave no refresh token (the app was already allowed once) | Remove Inbox Agent at myaccount.google.com/permissions, then sign in again |
| A sign-in stops working after seven days | The publishing status is Testing | Set it to **In production** |
| Microsoft says the app is not for this kind of account | The registration is not "Personal Microsoft accounts only", or the address is a work account | Fix the registration; work accounts use an app password if their administrator allows it |
| The 101st person cannot sign in with Google | The unverified-app limit of 100 users | They use an app password; or pay for Google's verification, and change [ADR-0014](../adr/0014-no-paid-services.md) first |

## Related

* [ADR-0015](../adr/0015-sign-in-with-google-and-microsoft-needs-the-owners-client-id.md)
* [Connect a mailbox](../guides/connect-a-mailbox.md)
* [The mailbox will not connect](mailbox-wont-connect.md)
* [Security model](../architecture/security-model.md)
