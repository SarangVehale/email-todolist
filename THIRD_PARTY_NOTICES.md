# Third-party notices

## Fonts (bundled in `app/ui/fonts`, SIL Open Font License 1.1; licence texts are alongside the files)

- Bodoni Moda: Indestructible Type
- IBM Plex Mono: IBM Corp.
- Space Grotesk: Florian Karsten

Obtained from the Fontsource packages (`@fontsource-variable/bodoni-moda`, `@fontsource/ibm-plex-mono`,
`@fontsource-variable/space-grotesk`).

## Code

The app is built with Tauri 2 and Rust crates (imap, mailparse, rusqlite with bundled SQLite, ureq,
keyring, qrcode, serde and others). Each is under its own permissive licence (MIT, Apache-2.0 or similar).
Generate the full list for a release with `cargo about` or `cargo license` before distributing.
