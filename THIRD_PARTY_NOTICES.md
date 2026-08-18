# Third-party notices

## Boa JavaScript engine

This repository contains a modified copy of `boa_engine` 0.21.1 under
`vendor/boa_engine`.

- Upstream project: <https://github.com/boa-dev/boa>
- Upstream revision: `bc36c3fac0969ea21ea0570b62e7846f97389b73`
- License: Unlicense OR MIT, at the recipient's option
- Local changes: [vendor/boa_engine/LOCAL_CHANGES.md](vendor/boa_engine/LOCAL_CHANGES.md)

The upstream license texts are preserved in
[vendor/boa_engine/LICENSE-MIT](vendor/boa_engine/LICENSE-MIT) and
[vendor/boa_engine/LICENSE-UNLICENSE](vendor/boa_engine/LICENSE-UNLICENSE).

## Renderer text dependencies

The release binary links the following renderer-only text crates resolved through Cargo:

- `fontique` 0.11.1, <https://github.com/linebender/parley>, Apache-2.0 OR MIT, for font
  discovery, matching, fallback, and in-memory font registration
- `harfrust` 0.5.2, <https://github.com/harfbuzz/harfrust>, MIT, for OpenType shaping
- `swash` 0.2.10, <https://github.com/dfrg/swash>, Apache-2.0 OR MIT, for glyph rasterization
- `unicode-bidi` 0.3.18, <https://github.com/servo/unicode-bidi>, MIT OR Apache-2.0
- `unicode-script` 0.5.8, <https://github.com/unicode-rs/unicode-script>, MIT OR Apache-2.0
- `unicode-segmentation` 1.13.3, <https://github.com/unicode-rs/unicode-segmentation>, MIT OR
  Apache-2.0

These crates are not vendored. Exact versions and their transitive dependency graph are recorded
in `Cargo.lock`; upstream license texts are distributed with the crates.

## Web Platform Tests parser fixtures

The curated tree-construction fixtures under `tests/html-parser/fixtures` include selected,
unmodified cases from web-platform-tests.

- Upstream project: <https://github.com/web-platform-tests/wpt>
- Upstream revision: `964ddae49acd35592ae4c2a50ea1b9fc2edec686`
- License: BSD-3-Clause
- Fixture-level provenance: [tests/html-parser/README.md](tests/html-parser/README.md)

The applicable license text is preserved in
[tests/html-parser/LICENSE-WPT.md](tests/html-parser/LICENSE-WPT.md).

## Fuzz-development tooling

The optional `fuzz` workspace uses the following development-only tools; neither is linked into the
browser's release binaries:

- `cargo-fuzz` 0.13.2, <https://github.com/rust-fuzz/cargo-fuzz>, MIT OR Apache-2.0
- `libfuzzer-sys` 0.4.13, <https://github.com/rust-fuzz/libfuzzer>,
  (MIT OR Apache-2.0) AND NCSA

Their versions are pinned by the workflow, fuzz manifest, and `fuzz/Cargo.lock`. All committed fuzz
corpus inputs were authored for this repository and are provided under the repository's MIT license.

Other Rust dependencies are resolved through Cargo and retain their respective
licenses. Their exact versions are recorded in `Cargo.lock`.

The `psl2` dependency embeds a compact, deterministically built snapshot of Mozilla's Public
Suffix List for cookie-domain and schemeful-site decisions. Its Rust code is available under MIT
OR Apache-2.0; the embedded list data is available under MPL-2.0. The exact crate and list versions
are pinned in `Cargo.lock` and exposed by `psl2::psl_version()`.
