# Release notes: version X.Y.Z

> **Status:** draft | current · **Audience:** people who install Inbox Agent · **Last reviewed:** YYYY-MM-DD

<!-- Copy this file when you prepare a release, fill it in from CHANGELOG.md (the headings below are the changelog's), and paste
     the result into the draft GitHub release before you publish it (RELEASING.md, "Before you publish the draft"). Write for the
     person installing, not for the maintainer: what they can do now, what to watch for, and what is not done. The changelog stays
     the record; these notes are its readable copy for one version. -->

One or two sentences: what this release is for, and whether it is a first build, a feature release or a fix. Say plainly if the
installers are unsigned.

## Added

* What a person can do now that they could not before, one line each, with the setting or key if there is one.

## Changed

* What behaves differently. Say what to expect, not what was edited.

## Fixed

* What was broken and is not any more, in the words a person would have used to describe it.

## Security

* What was found and fixed, said plainly, with the review that found it (see [SECURITY.md](../../SECURITY.md)). Say what an
  attacker could have done, not only what was changed.

## Known limitations

* What does not work or has not been verified (for example: not yet tried against every mail provider, no automatic updates,
  macOS asks you to open the app with right-click and Open until it is notarised).

## Downloads and checksums

The installers and `SHA256SUMS.txt` are attached to the GitHub release by the release workflow (it builds on Windows, macOS and
Linux, then writes the checksums and a build-provenance attestation). They are not stored in the repository.

| Platform | File | Signed | SHA-256 |
| --- | --- | --- | --- |
| Windows | name of the installer | yes or no | the checksum from `SHA256SUMS.txt` |
| macOS | name of the disk image | yes or no | |
| Linux | name of the package | not applicable | |

Verify a download before you run it:

```bash
sha256sum -c SHA256SUMS.txt
```

```powershell
Get-FileHash .\the-installer-file
```

Where the facts are kept once the release is published:

| Fact | Where it goes |
| --- | --- |
| What changed | [CHANGELOG.md](../../CHANGELOG.md), under a dated `## [X.Y.Z]` heading |
| Which build had what | [docs/VERSION_HISTORY.md](../VERSION_HISTORY.md) |
| What was published, signed or not, the checksums, the manual tests that passed, known problems | [docs/RELEASE_HISTORY.md](../RELEASE_HISTORY.md) |
| How the release was cut and checked | [RELEASING.md](../../RELEASING.md) |
