# Release history

What was actually published. A release is a git tag `v<version>` whose CI run produced installers and `SHA256SUMS.txt` on a draft
GitHub release that someone then published. Development milestones that were never released are in
[`VERSION_HISTORY.md`](VERSION_HISTORY.md); what changed in each version is in [`CHANGELOG.md`](../CHANGELOG.md).

**No release has been published yet.** The first one will be `0.1.0`.

## Published releases

| Version | Tag | Published | Platforms | Signed | Checksums and attestation | Live-mailbox tests | Known problems |
| --- | --- | --- | --- | --- | --- | --- | --- |
| _none yet_ | | | | | | | |

## How to add a row

`scripts/bump_version.py --check <version>` (which the release workflow runs on the tag) refuses a version that has no row in this
table, so the row is made when the release is prepared and finished after it is published:

1. **When preparing** (in the release commit, before tagging): add the row with `Published` = `in preparation`, the platforms CI will
   build, and `Signed` = `no` or `yes` according to the secrets you set. Leave the rest empty.
2. **After the CI run and your clean-machine checks** (`RELEASING.md`, "Before you publish the draft"): fill in the date you published,
   what was verified, the address of the `SHA256SUMS.txt` on the release, whether `gh attestation verify` passed, the result of the
   live-mailbox tests (`docs/TESTCASES.manual.md`, say which MT numbers passed, failed or were not run) and the problems you know of.
3. **If a release turns out to be bad**, do not delete its row. Change `Published` to `withdrawn` with the date, say why in
   `Known problems`, and add the fixing release below it (RELEASING.md, "If something is wrong after publishing").

Never record a release that was not published, and never edit the checksum of a published release.

## Row template

```
| <version> | v<version> | in preparation | Windows, macOS, Linux | no | | | |
```
