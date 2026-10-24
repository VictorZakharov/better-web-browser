# Crate layout

The browser was one Cargo package: about 15 MB of Rust in roughly 2,770 files. Rust compiles a crate as one unit, so an edit anywhere recompiled all of it, and the library's 4,000+ unit tests could only be rebuilt and run together. This note records the measured dependency graph between engine areas, the target crate layout, and the first split. Timings are in [Measurements](#measurements).

## Dependency graph

Counts are `crate::` path references from one area's files to another's, measured on `9ed98bab` with a small script (not committed). `super::` paths stay inside an area and are not counted. Sizes are source bytes.

| Area | Size | Main outgoing references |
| --- | ---: | --- |
| `engine::script` | 5.47 MB | fetch 163, dom 111, renderer_protocol 90, css 84, limits 71, page 35, font 30, storage 28, layout 21, image_decode 17, webgl 8, plus 22 smaller areas |
| `windows_app` (binary) | 1.84 MB | only the library's public API |
| `engine::webgl` | 1.53 MB | renderer_budget 9, image_decode 6, gpu_frame 4, process_memory 3, owner_cpu, limits, gpu_composition, renderer_protocol (1 each) |
| `engine::css` | 1.52 MB | dom 37, invalidation 20, limits 14, font 8, page 6, affine 6, layout 3 |
| `renderer_process` | 1.10 MB | renderer_protocol 233, script 70, limits 68, dom 53, fetch 34, page 28, layout 26, media 44, css 21 |
| `engine::layout` | 0.99 MB | page 29, css 23, dom 12, affine 12, invalidation 11 |
| `renderer_protocol` | 0.61 MB | limits 63, fetch 43, layout 11, storage 10, css 9 |
| `engine::page` | 0.51 MB | css 50, dom 33, limits 30, layout 27, fetch 26, font 19, invalidation 15, image_decode 12, script 5 |
| `engine::dom` | 0.29 MB | limits 11, pattern_eval 4, css 2 |
| `winhttp` | 0.20 MB | fetch 26, navigation 10, limits 8 |
| `engine::font` | 0.09 MB | css 15, limits 4, layout 3 |

Two facts decide the layout:

- **WebGL was already a leaf.** Its 15 outward references all point at small, dependency-free modules (budgets, limits, process memory, GPU frames, pixel arithmetic). Only `engine::script` uses it. It is 14% of the library and carries 534 of its unit tests.
- **DOM, CSS, layout, font and page form one cycle**, and `page` reaches `script`. The back-edges are few but structural: `dom` calls `css::selector_match::is_disabled` and `css::imports::SheetOverride`, and evaluates patterns through `pattern_eval`, which enters V8 (`script::engine::watchdog`). `css` names `page::Page` twice and `layout` once. `layout` uses `page::DecodedImage` (10), `page::Page` (4) and two media/canvas helpers. `image_decode` returns `page::DecodedImage`. Everything else points downward.

`renderer_process`, `renderer_protocol` and `script` depend on almost everything and change often, so they stay in the browser crate.

## Target layout

Crates are listed bottom-up. Each depends only on crates above it in this list, never on the browser crate, so an edit rebuilds that crate and its dependents, and its own unit tests build without the rest of the browser.

| Crate | Contents | State | Prerequisite |
| --- | --- | --- | --- |
| `breeze-platform` | limits, renderer budget, process/thread measurement, GPU frame and composition primitives, RGBA pixel arithmetic, the CPU-worker GPU guard | **done** | none |
| `breeze-webgl` | ANGLE-backed WebGL contexts and their native adapter | **done** | none |
| `breeze-codecs` | `encoded_audio`, `webm_opus`, `opus_audio`, `ogg_vorbis_headers`, `iso_bmff_audio`, `media_type` (about 0.25 MB) | next, mechanical | none: the cluster only references `limits` |
| `breeze-net` | `fetch`, `navigation`, `protocol_handlers`, `storage`, `cache_storage`, `indexed_db`, `text_decode`, `document` (about 0.4 MB), then `winhttp` above it | mechanical | break the `fetch` / `navigation` / `protocol_handlers` cycle by moving the scheme registry into `navigation` |
| `breeze-dom` | `engine::dom`, `invalidation`, `affine` (about 0.33 MB) | needs refactoring | replace the `pattern_eval` call with an injected pattern-matcher trait that `script` implements, and move `is_disabled` / `SheetOverride` into DOM |
| `breeze-style` | `engine::css`, `stylesheet`, `font` (about 1.6 MB) | needs refactoring | move `DecodedImage` and the intrinsic-size helpers to a resource module below CSS, and replace the `Page` references with a narrow style-host trait |
| `breeze-layout` | `engine::layout`, `display_list` (about 1 MB) | needs refactoring | as above, plus a `LayoutHost` trait for the four `Page` uses |
| `better-web-browser` | page, script, renderer protocol/process, media/capture processes, `winhttp` until it moves | remains | none |

Public paths are kept by re-export (`pub use breeze_platform::limits;`, `pub(crate) use breeze_webgl as webgl;`), so code and the binary still use `crate::limits::...` and `better_web_browser::gpu_composition::...`.

### Estimated rebuild per crate

The estimates scale the measured costs below by source size. They assume the measured rate holds; generic-heavy or test-heavy code compiles slower than the average.

The browser crate's own test loop has a large fixed cost: after any edit, the performance profile re-links one lib-test executable for the whole library, which took 24–33 s in every sample below, whichever area changed. A split-out crate escapes that loop: `breeze-webgl` (1.5 MB) rebuilt its own test executable in 3.1 s. Assuming a similar rate of about 2 s per MB of crate source, plus a fixed second for linking:

| Crate | Source | Estimated `cargo test -p <crate> --lib --no-run` after an edit |
| --- | ---: | ---: |
| `breeze-platform` | 0.03 MB | about 1 s (not measured) |
| `breeze-webgl` | 1.53 MB | **3.1 s measured** |
| `breeze-codecs` | 0.25 MB | 1–2 s |
| `breeze-net` | 0.4 MB (+0.2 MB `winhttp`) | 2–3 s |
| `breeze-dom` | 0.33 MB | 1–2 s |
| `breeze-style` | 1.6 MB | 3–5 s |
| `breeze-layout` | 1 MB | 3–4 s, rebuilding `breeze-style` only when style changes |
| `better-web-browser` | about 10 MB after all splits | 15–20 s: it still links every engine crate, plus V8 |

Each split removes its tests from the browser's lib-test executable, but the remaining browser crate stays the cost of any edit in `script`, `page` or the renderer. Getting those under 10 s also needs the browser's own tests split into smaller test targets.

## First split

`breeze-platform` and `breeze-webgl` were split first because they needed no redesign:

- `breeze-platform` takes `limits`, `renderer_budget`, `process_memory`, `gpu_frame` (now also defining `PresentedGpuCanvas`, re-exported by `renderer_protocol::gpu`), `gpu_composition`, `engine::owner_cpu`, the image decoder's `alpha`/`premultiply` byte arithmetic (as `pixels`), and `test_resources`. Items that were `pub(crate)` are now `pub`. `test_resources::require_native_gpu` is no longer `cfg(test)`, because a dependent crate's tests compile this crate without that cfg; it is `doc(hidden)` and production code never calls it.
- `breeze-webgl` is the former `src/engine/webgl` tree and `build/angle_api.rs`, moved with history. It exports only what `engine::script` uses: `Contexts` and its methods, `PixelReply`, the numeric command types and the `numeric_packet` decoder. `Contexts::snapshot` was test-only; it is now a `doc(hidden)` public method because the browser crate's WebGL tests read it.
- One test moved the other way: the check that every operation in `script/bootstrap/webgl_private_wire.js` is an admitted native command now lives in `engine::script::engine::bridge::gpu_numeric_tests`, beside the bootstrap it reads. The browser's bootstrap includes `crates/webgl/src/numeric_packet/operations.json` by path, because the native crate owns that table.
- The GPU command-sample measurement (an ignored hardware test) used `RasterImage::into_premultiplied_bgra` only to call `rgba_to_bgra`. It now calls `breeze_platform::pixels::rgba_to_bgra` directly, producing the same bytes.

`cfg(test)` is set only for the crate under test, never for its dependencies. Moved code that changed behaviour under `cfg(test)` therefore needed an explicit replacement, or the browser's tests would silently run production behaviour:

- `MAX_DOM_TREE_MUTATIONS_PER_TASK` and `MAX_SCRIPT_LOOP_ITERATIONS` shrink in tests. They stay in the browser crate's `limits` module, which re-exports everything else from `breeze_platform::limits`.
- WebGL unit tests use the WARP provider on every machine. `breeze_webgl::unit_test_backend::select()` makes later contexts in the process do the same; the browser's `HostState::new` and `WorkerHostState::new` call it in test builds, and every context the browser crate creates belongs to one of them.
- The CPU-worker GPU guard (`require_native_gpu`) now runs in every build before native device setup. It only reads `BREEZE_TEST_CPU_ONLY`, which only the full-test runner sets.

Before each future split, search the moved code for `cfg(test)` and `cfg!(test)` outside test modules.

The root manifest is now a workspace with `default-members` covering all three crates, so `cargo test`, `cargo clippy --all-targets`, `cargo fmt --all` and the release build cover them without extra flags. Shared dependency versions (`windows`, `windows-sys`, `mozangle`, `serde`, `serde_json`, `smallvec`) moved to `[workspace.dependencies]`. Feature unification still compiles each of them once.

Tooling changes:

- `run-full-tests.ps1` runs each workspace crate's test executable whole in the serial native-worker slot. An unknown workspace library fails the gate instead of being skipped.
- The impacted-test map assigns `crates/webgl/src/*` to the new `webgl-crate` group (`cargo test -p breeze-webgl --lib`) plus the existing browser and renderer WebGL groups. `crates/platform/*` selects the full suite, because every crate depends on it. Library groups now pass `-p better-web-browser` explicitly.
- `check-source-size.ps1` scans `crates/`. The third-party notices generator excludes all workspace members, not just the root package.

## Measurements

Workstation, 24 October 2026, toolchain 1.95.0, `-j 4`, `CARGO_INCREMENTAL=1`, warm target directories on G:, no other heavy build running. The baseline is `9ed98bab` in its own worktree and target directory. Each edit appends a private, unused function to the file, so it is a small edit; edits that change signatures used elsewhere rebuild more. Rows are single samples, with a second where the first looked off. Cold builds and the `release` lib-test build were not measured, at the user's request.

Performance profile, `cargo test --lib --no-run` (the browser library plus, after the split, the two new crates):

| Edit | Before | After |
| --- | ---: | ---: |
| `src/engine/layout/block/margins.rs` | 25.2 s | 24.4 s |
| `src/engine/css/cascade.rs` | 32.8 s, 28.0 s | 25.5 s, 24.2 s |
| WebGL `error_state.rs`, whole workspace | 26.7 s | 24.3 s |
| WebGL `error_state.rs`, `cargo test -p breeze-webgl --lib --no-run` | n/a (26.7 s) | **3.1 s** |

Development `cargo check --lib --tests` before the split took 18.6–20.2 s for the same layout and WebGL edits (five samples). Those samples may have overlapped another agent's builds.

The first `-p breeze-webgl` build in a target directory took 213 s once, because Cargo resolves features per selected package and rebuilt `windows` and ANGLE with WebGL's smaller feature set. Later edits reuse it.

## Cheaper options

These were not measured in this change: at the user's request the timing window was cut to a few minutes. Existing evidence and the reasons each one cannot reach 10 s on its own:

- **Linker (`rust-lld` / `lld-link`).** [Build performance](build-performance.md) measured a warm `performance` rebuild going from about 3.5 s to 3.01 s with `rust-lld` in August, on a much smaller tree. Linking is now a larger share of the 24 s browser test loop, so it deserves a fresh A/B (set `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=rust-lld`, which only relinks).
- **`incremental` and `codegen-units` in the `performance` profile.** The profile sets `incremental = false`. The earlier report measured 21.8 s after a comment edit and about 244 s after a real `margins.rs` edit with `CARGO_INCREMENTAL=1`; without incremental, every edit recompiles the whole crate. `codegen-units = 16` limits how much incremental reuse is possible. The next measurement is `--config profile.performance.package.better-web-browser.codegen-units=256` with `CARGO_INCREMENTAL=1`, which rebuilds only the browser crate.
- **Splitting test binaries.** This is what the crate split does for WebGL: its 534 tests now build and link without the browser library. Splitting the browser crate's remaining unit tests into several `tests/` targets would not help, because unit tests need the crate's private items and each target still links the full library.
