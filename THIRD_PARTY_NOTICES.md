# Third-party notices

## Fonts (bundled in `app/ui/fonts`, SIL Open Font License 1.1; licence texts are alongside the files)

- Bodoni Moda: Indestructible Type
- IBM Plex Mono: IBM Corp.
- Space Grotesk: Florian Karsten

Obtained from the Fontsource packages (`@fontsource-variable/bodoni-moda`, `@fontsource/ibm-plex-mono`,
`@fontsource-variable/space-grotesk`).

## Code

The app is built with Tauri 2 and about 565 Rust crates (imap, mailparse, rusqlite with bundled SQLite, ureq,
keyring, qrcode, serde and others). [`THIRD_PARTY_LICENSES.txt`](THIRD_PARTY_LICENSES.txt) names each crate with its licence and holds the
licence texts that came with them; the installer carries it next to the program, with this file and `LICENSE`.

`python scripts/gen_licenses.py` writes it from the crates' own source folders, and CI (`--check`) fails when it is out of date or when a
crate has a licence that is not on the reviewed list in the script. Today that list is MIT, Apache-2.0, the BSD licences, ISC, Zlib, 0BSD,
Unlicense, CC0, BSL-1.0, Unicode-3.0, CDLA-Permissive-2.0 (the list of trusted certificate authorities, as data) and MPL-2.0 (`cssparser`,
`cssparser-macros`, `selectors`, `dtoa-short` and `option-ext`, used unmodified through Tauri; their source is on crates.io). A crate that is only GPL, AGPL or LGPL stops the
build. A crate offering `MIT OR Apache-2.0 OR LGPL-2.1-or-later` is taken under MIT. Some crates ship no licence file; the list says so and gives
the licence they declare.
