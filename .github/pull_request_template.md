## What and why

<!-- One or two sentences. Link the design in docs/designs/ if there is one. -->

## Checklist

Documents are code: they change in the same pull request as the thing they describe.

- [ ] The documents that describe what I changed are updated (the table in `docs/runbooks/documentation-maintenance.md` says which), and `python scripts/check_docs.py` passes
- [ ] `CHANGELOG.md` is updated; `docs/VERSION_HISTORY.md` has the row (its commit hash goes in a follow-up commit); `docs/RELEASE_HISTORY.md` only for a real release
- [ ] New behaviour has a test that failed first, and each new guard was put back to its old behaviour once to see a test notice
- [ ] In `engine`: `cargo test --locked`, `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings` pass and the Rust documentation builds; in `app`: `npm test` passes
- [ ] No secret, real address or real mail in code, tests or documents
- [ ] For a phase of work: the council, the drift passes and the review loop are written down in `docs/<date>_PHASE<n>_REVIEW.md`
