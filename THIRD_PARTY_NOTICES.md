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

## Web Platform Tests parser fixtures

The curated tree-construction fixtures under `tests/html-parser/fixtures` include selected,
unmodified cases from web-platform-tests.

- Upstream project: <https://github.com/web-platform-tests/wpt>
- Upstream revision: `964ddae49acd35592ae4c2a50ea1b9fc2edec686`
- License: BSD-3-Clause
- Fixture-level provenance: [tests/html-parser/README.md](tests/html-parser/README.md)

The applicable license text is preserved in
[tests/html-parser/LICENSE-WPT.md](tests/html-parser/LICENSE-WPT.md).

Other Rust dependencies are resolved through Cargo and retain their respective
licenses. Their exact versions are recorded in `Cargo.lock`.
