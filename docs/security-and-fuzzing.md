# Hostile-input limits and fuzzing

Breeze treats all network and document bytes as hostile. The limits below are implementation
security boundaries, not web-platform compatibility targets. They live in `src/limits.rs` so a
review can audit the complete policy without hunting through parsers and platform adapters.

## Enforced budgets

| Boundary | Current limit | Behavior at the boundary |
|---|---:|---|
| HTML source | 8 MiB | Truncate at a UTF-8 boundary and retain an actionable document diagnostic. |
| DOM | 100,000 nodes; depth 512 | Stop streaming parser input, prune excess post-parse nodes iteratively, and reject script mutations that would exceed the budget. |
| HTML parse diagnostics | 256 | Retain the first errors and discard further diagnostics. |
| Rendered text | 2 MiB | Truncate Reader/document text before extending its backing string. |
| CSS | 2 MiB/source; 20,000 rules; nesting 64; 256 declarations/rule | Truncate source and stop bounded parser loops while retaining valid rules already parsed. |
| URLs | 16 KiB | Reject navigation addresses and URL resolution before parsing; embedded image data URLs have a separate 8 MiB boundary. |
| Fetch | 16 MiB/response; 64 KiB/preflight; 20 redirects | Abort streaming transport reads and reject excessive redirects. |
| Page resources | 32 MiB aggregate | Stop admitting additional fetched resource bodies for the document. |
| JavaScript | 8 MiB code; 32 dynamic scripts; 5,000,000 loop iterations | Reject excess code, stop dynamic-script admission, and terminate runaway Boa execution. Tests use a lower loop budget for fast regressions. |
| Script tasks | 10,000 DOM mutations; 128 timer callbacks/slice | Reject further host calls or yield the timer slice so one task cannot grow without bound. Unit tests use a lower mutation threshold for fast boundary regressions. |
| Raster images | 16 MiB encoded; 32,768 px/axis; 32 Mi pixels; 128 MiB decoded | Configure the `image` decoder's pre-allocation limits, then verify the exact pixel product before copying pixels. |
| SVG | 4 MiB source; 32 Mi pixels | Reject source before the third-party parser and reject dimensions before allocating the render pixmap. |
| Fonts | 32 MiB input/output; 256 WOFF tables | Validate container offsets, table counts, compressed sizes, and declared output size before allocation/decompression. |
| IPC | 256 KiB control frames; 8 MiB image frames; 16 KiB diagnostics | Reject frame headers before allocating payload buffers and truncate diagnostics at UTF-8 boundaries. |
| Renderer process | one child; 1 GiB process/job memory | A Windows Job Object prevents child creation and terminates the renderer when containment is lost or a budget is exceeded. |
| Renderer liveness | 10 s without heartbeat plus 2 s kill grace | Report unresponsive state, terminate the renderer Job automatically, preserve the browser process, and expose a reloadable crash surface. |

Network response limits apply to bytes delivered by the WinHTTP transport after its protocol
processing. Image, SVG, and font consumers enforce their own tighter decoded-form budgets as a
second boundary. The current page engine still runs in the browser process while renderer migration
continues; the renderer containment guarantees apply to the capability-free lifecycle child today.

## Fuzz targets

Six deterministic entry points exercise the highest-risk owned boundaries:

- full-document HTML tokenization/tree construction;
- fragment parsing and replacement;
- CSS stylesheet parsing and cascade construction;
- URL normalization and relative resolution;
- direct DOM mutation sequences; and
- JavaScript DOM-host bindings.

The target implementations are shared between libFuzzer and `tests/hostile_input.rs`. Stable Windows
CI replays every committed seed on each source pull request; the scheduled/manual Linux workflow
runs true coverage-guided libFuzzer campaigns with Rust nightly 2026-08-15, `cargo-fuzz` 0.13.2, and
`libfuzzer-sys` 0.4.13. Each input has a five-second timeout and each target has a 1 GiB RSS limit;
either boundary is a failing finding. See `fuzz/README.md` for exact commands.

Checked-in seeds are authored for this repository. A discovered crash or hang is not considered
fixed until its minimized input is committed to the corresponding corpus and succeeds under stable
regression replay. Fuzz artifacts, coverage output, and build output stay ignored.

## Review checklist

When adding an input boundary or decoder:

1. Add a named limit to `src/limits.rs` and document rejection or truncation behavior here.
2. Reject impossible sizes and counts before expensive parsing, allocation, or decompression.
3. Prefer one bounded parse; do not parse untrusted bytes twice merely to obtain metadata.
4. Add a deterministic hostile-input regression and extend a fuzz target when applicable.
5. Keep user-visible failures actionable and fail the resource or renderer without crashing the UI.
