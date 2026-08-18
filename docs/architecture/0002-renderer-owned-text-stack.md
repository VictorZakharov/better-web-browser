# ADR 0002: Renderer-owned Rust text stack

- Status: Accepted
- Date: 2026-08-18
- Issue: [#43](https://github.com/VictorZakharov/better-web-browser/issues/43)

## Context

The renderer owns hostile HTML, CSS, script, image, SVG, and webfont input. The earlier migration
still measured document text with GDI and allowed the privileged browser to choose the final font.
That split made layout disagree with painting and made remote-font support difficult to contain:
either font bytes or font-table parsing would eventually cross into the browser.

The target boundary is stricter:

- remote font bytes and page text stay in the AppContainer renderer;
- font discovery, matching, shaping, fallback, and rasterization happen in the renderer;
- presentation IPC contains only bounded, pointer-free glyph placements and raster assets; and
- the browser validates those values and composites pixels without parsing fonts or measuring text.

The implementation must support real web text rather than an ASCII-only fast path. In particular,
CSS font selection, combining sequences, ligatures, emoji fallback, Arabic and other right-to-left
text, Devanagari, and mixed-direction runs are compatibility requirements.

## Decision

Use [COSMIC Text 0.19](https://github.com/pop-os/cosmic-text) with its Swash rasterizer inside each
renderer. Enable `std`, `swash`, and `shape-run-cache`; disable unrelated default features.
Use `Shaping::Advanced` for document text.

The renderer translates the implemented CSS `font-family`, `font-size`, `font-weight`,
`font-style`, `letter-spacing`, and `word-spacing` values into a reusable COSMIC buffer. It
caches bounded shaped results by text and font specification, rasterizes glyphs once per renderer
glyph epoch, and gives every shaped run a stable nonzero ID.

The presentation protocol carries:

- a bounded table of premultiplied BGRA or alpha-mask glyph rasters;
- finite CSS-pixel placements that reference those raster IDs;
- a raster-run ID and glyph epoch so the browser can safely reuse composed surfaces; and
- cheap shape-cache counts for benchmark diagnosis.

The browser rejects non-finite geometry, oversized dimensions, excessive pixels, excessive glyph
counts, and payloads beyond the shared protocol budgets. It refuses to paint unknown resource IDs
or mismatched color modes. Its composed-run cache is capped at 4,096 entries and 24 MiB. Eviction
occurs before allocating a replacement DIB. Navigation, DPI changes, and webfont-set changes
advance the glyph epoch and clear stale renderer state.

Trusted `browser.local` UI text may continue to use the platform fallback painter. Remote
documents may not.

## Candidate evaluation

The two candidates were measured with small release-mode spikes on this Windows machine. Clean and
incremental times are wall-clock Cargo build times; discovery and shaping are process-local
microbenchmarks. Spike binary sizes are useful for comparing candidates but are not production
binary deltas.

| Candidate | Clean build | Incremental build | Spike binary | Font discovery | Faces | Mean shape | Raster included |
|---|---:|---:|---:|---:|---:|---:|---|
| Parley 0.11 | 22.821 s | 0.242 s | 1,343,488 B | 1.493 ms | n/a | 7.460 us | No |
| COSMIC Text 0.19 | 21.749 s | 0.208 s | 2,582,528 B | 126.670 ms | 465 | 17.062 us | Yes, Swash |

Parley shaped faster and produced the smaller spike. It was not selected because this boundary also
needs rasterization: adopting Parley would require a second font/raster stack plus ownership and
cache integration between them. COSMIC provides discovery, fallback, bidirectional/complex
shaping, in-memory fonts, rasterization, and color emoji behind one bounded adapter. That reduced
security-boundary code and delivered the required scripts in the available implementation window.

This does not reject Parley permanently. A measured Parley plus renderer-owned rasterizer prototype
is a valid future option if cold shaping remains the dominant page-ready cost.

## Why advanced shaping is mandatory

COSMIC's official `Shaping` documentation says that `Basic` has no font fallback and does not
display complex scripts correctly. It is intended for applications that completely control both
text and font. Web content satisfies none of those constraints, so selecting it based on an ASCII
scan would create language-dependent correctness and security behavior. `Advanced` is therefore
the only document-text mode.

This choice also follows:

- [CSS Fonts Module Level 4](https://www.w3.org/TR/css-fonts-4/), including ordered family and
  cluster matching;
- [CSS Text Module Level 3](https://www.w3.org/TR/css-text-3/) for spacing and inline text behavior;
  and
- [Unicode Standard Annex #9](https://www.unicode.org/reports/tr9/) for bidirectional display.

## Performance evidence

All page measurements used the release build, a 1,920 by 1,080 hidden window, 100 ms settle, and the
same Wikipedia earthquake article. Breeze runs used
`scripts/run-hidden-benchmark.ps1`, whose fail-closed guard verifies `--benchmark` in the actual
child command line and sets `CreateNoWindow`.

### Baselines

The same-day `origin/main` control was rebuilt from archived source and run three times. A second
frozen pre-change batch is retained because it was captured before any issue-43 work.

| Baseline (3-run median) | Page ready | Non-network | Layout/paint | Scroll frame median / p95 | Worst input latency | Working set |
|---|---:|---:|---:|---:|---:|---:|
| Frozen pre-change | 545.780 ms | n/a | n/a | 5.138 / 9.134 ms | 11.130 ms | n/a |
| Same-day `origin/main` | 576.915 ms | 223.193 ms | 105.540 ms | 6.817 / 10.467 ms | 14.772 ms | 155.7 MiB |

The old hidden harness repainted the full 1,920 by 1,080 surface for every scroll sample. During
this work it was corrected to retain and shift offscreen pixels and paint only the exposed strip,
matching the interactive `ScrollWindowEx` path. Cold page-ready remains directly comparable.
Scroll values before and after that correction describe the actual behavior of their respective
builds but are not a like-for-like paint microbenchmark.

### Intermediate attempts

`n=3` rows are medians; `n=1` rows are explicitly diagnostic and are not treated as stable
performance claims.

| Attempt | Runs | Page ready | Layout/paint | Scroll frame median / p95 | Worst input latency | Working set | Result |
|---|---:|---:|---:|---:|---:|---:|---|
| Per-glyph browser composition | 3 | 727.906 ms | n/a | 50.861 / 79.661 ms | about 10-11 s | n/a | Rejected |
| Batched unbounded run cache | 1 | 769.217 ms | 207.008 ms | 14.182 / 23.652 ms | 446.408 ms | 166.1 MiB | Rejected |
| Reused browser DC only | 1 | 801.307 ms | 197.596 ms | 26.866 / 39.174 ms | 3,496.440 ms | 85.1 MiB | Rejected |
| Bounded 24 MiB run cache | 3 | 712.560 ms | 222.137 ms | 12.793 / 24.261 ms | 255.621 ms | 164.6 MiB | Kept cache, paint still failed |
| Reusable COSMIC buffer | 1 | 715.959 ms | 271.438 ms | 10.955 / 17.170 ms | 31.970 ms | 181.6 MiB | Kept buffer |
| 64 MiB run cache | 3 | 933.101 ms | 328.433 ms | 12.888 / 24.514 ms | 389.737 ms | 172.1 MiB | Rejected |
| Retained-strip hidden scrolling | 1 | 734.910 ms | 254.032 ms | 2.257 / 3.453 ms | 8.357 ms | 173.1 MiB | Kept |
| Shared `Arc` glyph slices | 1 | 848.948 ms | 254.232 ms | 2.177 / 3.288 ms | 10.155 ms | 176.2 MiB | Rejected |
| Borrowed allocation-free cache hits | 1 | 814.444 ms | 255.588 ms | 2.008 / 2.926 ms | 6.992 ms | 174.0 MiB | Rejected |
| COSMIC `BufferLine` reuse | 1 | 736.456 ms | 255.279 ms | 1.998 / 3.218 ms | 7.272 ms | 173.1 MiB | Rejected |
| Count telemetry | 1 | 821.362 ms | 252.205 ms | 2.309 / 3.855 ms | 10.623 ms | 175.7 MiB | Kept counts |
| Timing probe | 1 | 733.095 ms | 275.621 ms | 1.972 / 2.870 ms | 7.929 ms | 177.9 MiB | Diagnostic only; removed |
| COSMIC inner cache disabled | 1 | 824.317 ms | 260.954 ms | 1.981 / 2.785 ms | 8.349 ms | 170.9 MiB | Rejected |

The timing probe observed about 102 ms across approximately 393,720 cache hits and 175 ms across
2,664 cold misses. Disabling COSMIC's inner shape-run cache slightly increased cold-miss time, so
the feature remains enabled. The probe's per-call timing was removed from production; count
telemetry remains.

### Selected implementation

| Metric | Run 1 | Run 2 | Run 3 | Median |
|---|---:|---:|---:|---:|
| Page ready | 752.234 ms | 595.118 ms | 714.472 ms | 714.472 ms |
| Network | 428.990 ms | 298.936 ms | 393.068 ms | 393.068 ms |
| Non-network | 323.244 ms | 296.182 ms | 321.404 ms | 321.404 ms |
| Layout/paint | 239.169 ms | 210.863 ms | 214.886 ms | 214.886 ms |
| Text cache hits | 398,997 | 398,682 | 398,997 | 398,997 |
| Text cache misses | 2,684 | 2,679 | 2,684 | 2,684 |
| Scroll frame median | 1.791 ms | 1.797 ms | 1.798 ms | 1.797 ms |
| Scroll frame p95 | 2.676 ms | 2.717 ms | 2.703 ms | 2.703 ms |
| Worst input latency | 11.122 ms | 9.067 ms | 9.640 ms | 9.640 ms |
| Working set | 178.598 MiB | 179.910 MiB | 179.668 MiB | 179.668 MiB |

All three final scroll traces passed the acceptance thresholds and reached the stable range at
256 ms. Against the same-day control, page-ready is 23.8% slower, non-network time is 44.0% slower,
layout/paint is 103.6% slower, and working set is 15.4% higher. The release executable grew from
15,661,568 B to 17,833,984 B (+13.87%).

This exceeds issue #43's 10% page-ready allowance. The security/correctness boundary is accepted
only with that regression explicitly visible for review: approximately 2,684 unique advanced
shapes and rasters add the cold cost, while hundreds of thousands of repeated measurements hit the
cache. [Issue #60](https://github.com/VictorZakharov/better-web-browser/issues/60) will evaluate
lazy or parallel cold rasterization and a Parley-plus-raster prototype without weakening
complex-script behavior or moving font parsing into the browser.

## Verification contract

Tests cover:

- deterministic geometry for Latin, Arabic/RTL, Devanagari, combining marks, ligatures, emoji
  fallback, and mixed-direction text;
- in-memory webfont aliases inside the actual AppContainer renderer;
- protocol round trips plus malformed, oversized, duplicate, unknown, and color-mismatched glyph
  assets;
- resource-epoch advancement and raster republication across navigation;
- retained-scroll exposed-strip damage calculations; and
- the fail-closed hidden benchmark launch guard.

The final hidden Wikipedia capture was inspected at the target viewport and contained shaped text
without the earlier overlap artifacts. Renderer-process and live-runtime integration tests retain
their hidden `CREATE_NO_WINDOW` launch paths.

## Consequences

- Font parsing and page-text shaping no longer increase the browser process's attack surface.
- Layout and painting use one renderer-owned result, eliminating the GDI measurement/final-font
  mismatch.
- Complex scripts, fallback, combining marks, ligatures, and color emoji have one standards-oriented
  path.
- IPC and browser memory grow with bounded glyph assets; glyph epochs and cache ceilings make that
  growth explicit.
- Cold page readiness and renderer working set regress materially and remain optimization work.
- CSS Fonts, line breaking, vertical text, variable-font controls, selection, and other broader
  text behavior are still incomplete; this ADR defines ownership, not full conformance.

## Dependency provenance

COSMIC Text is distributed under MIT OR Apache-2.0. It is resolved through Cargo rather than
vendored, and its exact version and transitive dependency graph are locked in `Cargo.lock`.
`THIRD_PARTY_NOTICES.md` records the direct dependency and upstream project.
