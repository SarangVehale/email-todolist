# Install the app, and what to do when Windows warns you

> **Status:** current · **Audience:** anyone installing Inbox Agent for themselves or for someone else · **Last reviewed:** 2026-11-15

By the end of this guide the app will be installed and open, even though Windows (or macOS) shows a warning first. It takes about five minutes.

## What you need

* The installer for your computer from the project's Releases page: a file ending in `.exe` (Windows), `.dmg` (macOS) or `.AppImage` or `.deb`
  (Linux).
* Nothing else. The app asks for your email address when it first opens ([connect a mailbox](connect-a-mailbox.md)).

## Why there is a warning

The builds are **not signed** with a paid certificate (the project uses no paid service: [ADR-0014](../adr/0014-no-paid-services.md)). Windows and macOS
warn about any program from a publisher they do not know. The warning is about who made the file, not about a problem found in it. Check the
download against the checksums on the Releases page (the file `SHA256SUMS.txt` lists a SHA-256 for each installer) if you want to be sure it is the one that was published.

## Steps

### Windows

1. Open the installer you downloaded.
2. A blue window says **Windows protected your PC**. It has only a **Don't run** button. Press the words **More info** (a small link in the window).
3. A second button, **Run anyway**, appears. Press it.
4. The installer asks where to put the app. Press **Next** until it says it is finished. The app is in the Start menu as **Inbox Agent**.

### macOS

1. Open the `.dmg` and drag Inbox Agent into **Applications**.
2. The first time, macOS says it cannot verify the app. Open **System Settings**, **Privacy & Security**, scroll to the message about Inbox Agent and press **Open Anyway**.

### Linux

An `.AppImage` needs to be made runnable once: right-click it, **Properties**, **Permissions**, tick **Allow executing file as a program**. A `.deb` is
installed from the software centre in the usual way.

## Check that it worked

The window **Set up Inbox Agent** opens with **Welcome.** If it does, go on with [connect a mailbox](connect-a-mailbox.md).

## If something goes wrong

| What you see | Why | What to do |
| --- | --- | --- |
| The warning has no **More info** link | Your computer is managed by an organisation that blocks unknown programs | Ask whoever manages it, or use the app on a computer you manage |
| Your antivirus removes the file | An unsigned program from an unknown publisher | Check the checksum, then allow it in the antivirus |
| The app opens and closes at once | A damaged download | Download it again and compare the checksum |

## Notes

The pictures of these screens are not here: they have to be taken on a real computer and the project has not done that yet. Free signing for open-source
projects (SignPath Foundation) is an application that only the owner can make; when it is granted the warning goes away and this guide shrinks.

## Related

* [Connect a mailbox](connect-a-mailbox.md)
* [Cut a release](../runbooks/cut-a-release.md)
* [Design: Phase 12](../designs/phase12.md)
