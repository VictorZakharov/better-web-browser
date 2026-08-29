# Development-only third-party material

`THIRD_PARTY_NOTICES.md` is intentionally limited to the locked Windows x64 graph shipped in the browser archive. The following material is used to develop or test the repository and is not linked into that release binary.

## Web Platform Tests parser fixtures

Selected unmodified tree-construction cases under `tests/html-parser/fixtures` come from [web-platform-tests](https://github.com/web-platform-tests/wpt) revision `964ddae49acd35592ae4c2a50ea1b9fc2edec686` under BSD-3-Clause. Fixture-level provenance and the applicable license are preserved in `tests/html-parser/README.md` and `tests/html-parser/LICENSE-WPT.md`.

The owned H.264/AAC integration fixture under `tests/fixtures/media` is the unmodified
`media/test-1s.mp4` from web-platform-tests revision
`322ebb726e0bc6ee05c5635f2978e3175dd781b9`, stored as Base64 text. Its pinned URL, decoded hash,
size, and BSD-3-Clause license are preserved beside the fixture.

The expanded curated WPT runner fetches sparse upstream files at the separately pinned revision in `tests/wpt/manifest.json`; those files remain outside release archives.

## Fuzz tooling

The optional `fuzz` workspace pins `cargo-fuzz` 0.13.2 (MIT OR Apache-2.0) and `libfuzzer-sys` 0.4.13 ((MIT OR Apache-2.0) AND NCSA). Neither is linked into or included with the browser release. All checked-in fuzz corpus inputs were authored for this repository and use the repository MIT license.
