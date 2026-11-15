# Releasing Inbox Agent

A release is a git tag. CI tests everything, builds the installers for Windows, macOS and Linux, and attaches them,
with a `SHA256SUMS.txt`, to a **draft** GitHub release. Nothing is public until you publish the draft.

## One-time setup (needs your accounts)

1. **GitHub repository.** Create it (private is fine to start), push this project, and turn on:
   Settings → Actions → "Read and write permissions"; Security → Dependabot alerts and security updates; branch
   protection on `main` requiring the `CI` checks.
2. **Licence.** MIT (see `LICENSE`; the copyright line says "Inbox Agent contributors", change it to your name or
   company if you prefer). Third-party notices are in `THIRD_PARTY_NOTICES.md`.
3. **Add a security contact** to `SECURITY.md` ("Reporting a vulnerability").
4. **Code-signing certificates.** Without them users see warnings (Windows SmartScreen, macOS Gatekeeper blocks the
   app). Builds still work unsigned; CI prints a warning.

   | Platform | What you need | Repository secrets to add |
   | --- | --- | --- |
   | Windows | An Authenticode certificate (an EV certificate removes SmartScreen warnings immediately; an OV certificate earns reputation over time) exported as a `.pfx` | `WINDOWS_CERTIFICATE` (the `.pfx` as base64: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`), `WINDOWS_CERTIFICATE_PASSWORD` |
   | macOS | An Apple Developer account ($99/yr), a "Developer ID Application" certificate exported as `.p12`, and an app-specific password for notarization | `APPLE_CERTIFICATE` (`.p12` as base64), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` |
   | Linux | Nothing required | - |

   Never commit certificates or passwords; secrets live only in GitHub → Settings → Secrets and variables → Actions.

## Cutting a release

```bash
# 1. Decide the version, then set it everywhere (package.json, tauri.conf.json, both Cargo.toml and both Cargo.lock)
python scripts/bump_version.py 0.1.0

# 2. Write the changelog: rename "Unreleased" to the date and make sure the notes are true
#    (CHANGELOG.md must contain a dated "## [0.1.0]" heading; CI checks this)
#    Add the version's row to docs/RELEASE_HISTORY.md ("in preparation"; CI refuses a tag without it)
#    and its milestone to docs/VERSION_HISTORY.md

# 3. Everything must be green locally
cd engine && cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked && cd ..
(cd engine && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps)   # the Rust documentation builds
(cd app && npm test)                      # the window's own tests, in a real browser
python scripts/test_check_docs.py && python scripts/check_docs.py    # links, diagrams, the command reference, ADRs, doc comments
python scripts/gen_testcases.py          # keeps docs/TESTCASES.md current
python scripts/bump_version.py --check 0.1.0      # the release checks too: a dated changelog heading, a release-history row
python scripts/test_bump_version.py
python scripts/test_gen_licenses.py && (cd app/src-tauri && cargo fetch --locked) && python scripts/gen_licenses.py --check   # the licence list is current and every licence is reviewed

# 4. Commit, tag, push the tag (the tag triggers the release workflow)
git add -A && git commit -m "Release 0.1.0"
git tag v0.1.0 && git push && git push origin v0.1.0
```

The workflow then runs: **verify** (tag = every version field = a dated changelog entry = a row in `docs/RELEASE_HISTORY.md`) → **test** on Linux, Windows and
macOS (a red test stops the release) → **build** the installers one after the other → **checksums** (`SHA256SUMS.txt` plus a
build-provenance attestation). Tags containing a dash (`v0.2.0-beta.1`) become pre-releases.

## What the rehearsals found

`release.yml` was rehearsed six times on 2026-11-14 and 2026-11-15, each with a throwaway tag (`v0.1.0-rc.1` to `-rc.6`, on branches
that were never merged). **The last four ran to the end** (22 to 32 minutes): `verify`, the three `test` jobs, the builds and `checksums`. The
first two failed, and each showed something real.

*Rehearsal 1:* `verify`, the three `test` jobs and the Windows and Linux builds passed. **The macOS build failed**, and the cause was found
by building the same thing on a branch that reported its own output: the workflow passed `APPLE_CERTIFICATE` and the other signing variables from secrets
that did not exist, and an unset secret reaches a step as an *empty string*, not as nothing. Tauri takes an empty `APPLE_CERTIFICATE`
for a certificate, tries to import it and stops ("failed to import the signing certificate"). Now the variables are passed only by the
step that runs when the secrets exist; the unsigned step carries none. The `build` jobs also run one at a time now, because each finds or
creates the draft release for the tag and two doing that together may make two drafts.

*Rehearsal 2:* the Ubuntu `test` job failed in the window's tests, in a part of the harness that had nothing to do with the release: it
waited a fixed 15 seconds for the browser to open its debugging port, which is too short on a busy runner (CI had failed twice the same way).
Fixed, with a test.

*Rehearsal 3:* everything passed. `gh release download <tag>` and `gh release upload <tag>` work on a release that is still a draft, the
attestation step ran, and the `checksums` job's annotation ("Files on the draft release") listed what a release carries: the Windows
installer (`..._x64-setup.exe`), a universal macOS disk image and app archive (`..._universal.dmg`, `..._universal.app.tar.gz`) and Linux
`.deb` and AppImage packages, all on one draft. That annotation can be read without signing in. Nothing was opened or run on a Mac or a Linux
machine, and the builds are unsigned. *Rehearsal 4:* the same, with `tauri-apps/tauri-action` 1.0 and `actions/attest-build-provenance` 4.2 (the two Dependabot updates, which
change this workflow); it passed in 22 minutes, the macOS archive now carries the version in its name, and the attestation step's Node 20 notice
is gone. Both are taken.

*Rehearsal 5* ran on the tree of commit `41595fd` (after the first review of this work, with the licence files in the installers): it passed in 27
minutes, and the `checksums` job's new step, which looks inside the `.deb`, the `.app.tar.gz` and the AppImage for `LICENSE`,
`THIRD_PARTY_NOTICES.md` and `THIRD_PARTY_LICENSES.txt`, reported "missing: none" (an annotation, "Licence files inside the packages").
*Rehearsal 6* ran on the tree of commit `667c28e` (the end of Phase 12: the engine's new `open` crate, the second licence check on a regenerated `THIRD_PARTY_LICENSES.txt`, the
workflow's environment-variable change and the AppImage message, none of which had been through a rehearsal): it passed in 31 minutes. `verify`, the three `test` jobs, the three builds and `checksums` all
succeeded; the draft held the `.exe`, the universal `.dmg` and `.app.tar.gz`, the `.AppImage` and the `.deb`, and the package check reported "missing: none". The tag and the branch were deleted
afterwards; the draft release is the owner's to delete (it needs a sign-in).
What no rehearsal can show: the signed paths (Apple notarization, the Windows certificate), a real tag, a published release, and anything
about installing or running the macOS and Linux packages. Two small changes came after it (the AppImage step now says when extraction fails, and the scale check runs the app with its keychain in
memory); rehearsal 6 covered both. Rehearse again after any later change to the workflow.

To rehearse again: `python scripts/bump_version.py 0.1.0-rc.7` on a branch, a dated `## [0.1.0-rc.7]` changelog heading and a row in
`docs/RELEASE_HISTORY.md`, then tag `v0.1.0-rc.7`. It becomes a pre-release draft. Afterwards delete the draft (on the Releases page: it
needs a sign-in), the tag and the branch. A rehearsal builds three installers one after the other, so it takes about half an hour.

## Before you publish the draft

Do this on **clean machines** (a fresh Windows user or VM, a Mac, a Linux box), not your development computer:

1. Download the installer from the draft release. Verify it: `sha256sum -c SHA256SUMS.txt` (Windows:
   `Get-FileHash <file>`), and optionally `gh attestation verify <file> --repo <owner>/<repo>`.
2. Install, run the wizard, and work through the **live-mailbox plan** in `docs/TESTCASES.md` (sections A-D at minimum).
3. Check the platform specifics: Windows shows the publisher name (signed) and no SmartScreen block; macOS opens
   without a Gatekeeper block (`spctl -a -vv "Inbox Agent.app"`); Linux AppImage/deb launches and the tray icon shows.
4. Uninstall and confirm it leaves nothing running.
5. Edit the release notes (copy from `CHANGELOG.md`), then click **Publish**.
6. Finish the release's row in `docs/RELEASE_HISTORY.md` (the date, whether it is signed, the checksums, which manual tests passed,
   known problems), commit it, and push. A release that is withdrawn keeps its row, marked withdrawn.

## If something is wrong after publishing

- **A broken build:** mark the release as pre-release or delete it (users who already installed keep their copy), fix,
  bump the patch version (`0.1.1`) and release again. Never re-tag a published version.
- **A security problem:** follow `SECURITY.md`, ship a patch release, and say plainly in the notes what was fixed.
- **A bad dependency:** Dependabot opens update PRs; CI's `audit` job fails the build on a vulnerable dependency.

## Switching on the update notice

The app can tell users when a newer version exists. It is off until you name the repository: set `UPDATE_REPO` in
`engine/src/updates.rs` to `"owner/name"` (and the `repository` field in `app/src-tauri/Cargo.toml`; add one to `engine/Cargo.toml` and `app/package.json` if you want it there too),
rebuild, and tag a release. From then on the app asks GitHub once a day and shows a notice with a link to the release page. It
never downloads or installs anything, so it needs no signing keys. The test `the_check_is_off_until_a_repository_is_set`
documents the switch; update it when you flip it. Release tags must look like `v1.2.3` (an optional short suffix such as
`-rc.1` is allowed); anything else is ignored by the check.

## Free code signing

Unsigned installers work but show warnings (Windows SmartScreen, macOS Gatekeeper). Paying for certificates is not
needed to start. As far as we know, the SignPath Foundation offers free Windows code signing to open-source projects; it
needs a public repository, builds done in CI, and an application on their site, so it is a step for the repository owner
(check their current terms). macOS notarization requires an Apple Developer account, which is paid; until then macOS users
must open the app with right-click, Open. Say so in the release notes.

## What is not set up yet

- **Automatic updates.** Users must download new versions by hand. To add them: generate an updater key pair
  (`npx tauri signer generate`), keep the private key as a secret, add `tauri-plugin-updater` with the public key and a
  `latest.json` endpoint on the GitHub release. This needs your repository URL, so it was left out rather than guessed.
- **Reproducible builds** and an **SBOM**. The checksums and build-provenance attestation cover integrity today.
- **A custom update/download site.** Releases are distributed from GitHub Releases.

## Data and uninstall notes for users

Uninstalling removes the program but keeps the user's data folder (`app.inboxagent.desktop` under their local application-data
folder, `%LOCALAPPDATA%` on Windows) and their keychain entries. "Settings → Sign out and reset" removes the password and local data first. See
`PRIVACY.md`.
