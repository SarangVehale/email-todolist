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
#    (CHANGELOG.md must contain a "## [0.1.0]" heading; CI checks this)

# 3. Everything must be green locally
cd engine && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cd ..
python scripts/gen_testcases.py          # keeps docs/TESTCASES.md current
python scripts/bump_version.py --check

# 4. Commit, tag, push the tag (the tag triggers the release workflow)
git add -A && git commit -m "Release 0.1.0"
git tag v0.1.0 && git push && git push origin v0.1.0
```

The workflow then runs: **verify** (tag = every version field = changelog entry) → **test** on Linux, Windows and
macOS (a red test stops the release) → **build** the installers in parallel → **checksums** (`SHA256SUMS.txt` plus a
build-provenance attestation). Tags containing a dash (`v0.2.0-beta.1`) become pre-releases.

## Before you publish the draft

Do this on **clean machines** (a fresh Windows user or VM, a Mac, a Linux box), not your development computer:

1. Download the installer from the draft release. Verify it: `sha256sum -c SHA256SUMS.txt` (Windows:
   `Get-FileHash <file>`), and optionally `gh attestation verify <file> --repo <owner>/<repo>`.
2. Install, run the wizard, and work through the **live-mailbox plan** in `docs/TESTCASES.md` (sections A-D at minimum).
3. Check the platform specifics: Windows shows the publisher name (signed) and no SmartScreen block; macOS opens
   without a Gatekeeper block (`spctl -a -vv "Inbox Agent.app"`); Linux AppImage/deb launches and the tray icon shows.
4. Uninstall and confirm it leaves nothing running.
5. Edit the release notes (copy from `CHANGELOG.md`), then click **Publish**.

## If something is wrong after publishing

- **A broken build:** mark the release as pre-release or delete it (users who already installed keep their copy), fix,
  bump the patch version (`0.1.1`) and release again. Never re-tag a published version.
- **A security problem:** follow `SECURITY.md`, ship a patch release, and say plainly in the notes what was fixed.
- **A bad dependency:** Dependabot opens update PRs; CI's `audit` job fails the build on a vulnerable dependency.

## Switching on the update notice

The app can tell users when a newer version exists. It is off until you name the repository: set `UPDATE_REPO` in
`engine/src/updates.rs` to `"owner/name"` (and the `repository` fields in the two `Cargo.toml` files and `package.json`),
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
