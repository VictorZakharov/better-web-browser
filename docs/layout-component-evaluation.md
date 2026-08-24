# CSS and layout component evaluation

- Status: Complete
- Date: 2026-08-24
- Issue: [#54](https://github.com/VictorZakharov/better-web-browser/issues/54)

## Decision

Do not add Taffy, Stylo, or Blitz to Breeze's production dependency graph for the technical alpha.

- **Taffy: adapt later.** Its low-level API is the best fit for Breeze, but only after box geometry is
  represented independently from display-list construction. A flex adapter probe showed that a
  solver-only substitution would leave allocated flex sizes disconnected from painted child boxes.
- **Stylo: defer.** It is browser-grade and actively maintained, but adopting it replaces Breeze's
  computed-style model, cascade, invalidation, and DOM/style traits rather than one CSS parser.
- **Blitz: reject as a dependency; retain as a reference.** Its integration of Stylo, Taffy, Parley,
  and AccessKit is useful prior art, but `blitz-dom` owns the same DOM/style/layout/text layers that
  Breeze must keep behind its renderer process and typed presentation boundary.

This is a time-boxed no-go for alpha, not a rejection of reusable layout algorithms. No dependency,
copied code, site-specific behavior, or benchmark-only production path was added.

## Existing ownership boundary

Breeze currently computes styles into `StyleSet`, then performs layout, text measurement, and
display-list construction together in `LayoutEngine`. Flex and grid sizing call back into block and
inline layout while output items are appended. The renderer serializes the bounded `LayoutOutput`;
the browser validates and paints it.

That design keeps hostile document state and text inside the renderer, but it is not the reusable
box tree expected by Taffy or the DOM/style integration expected by Stylo. A candidate must preserve:

- renderer ownership of DOM, CSS, fonts, text, and layout;
- bounded presentation IPC with no browser-side document tree;
- stable DOM identity and document/session revision checks;
- existing incremental style invalidation and retained display-list behavior; and
- CSS inline formatting, tables, replaced elements, forms, SVG, floats, and positioned content
  while another algorithm owns block, flex, or grid geometry.

The relevant platform contracts are [CSS Display](https://www.w3.org/TR/css-display-3/),
[CSS Box Alignment](https://www.w3.org/TR/css-align-3/),
[CSS Flexible Box Layout](https://www.w3.org/TR/css-flexbox-1/),
[CSS Grid Layout](https://www.w3.org/TR/css-grid-2/), and
[CSS Text](https://www.w3.org/TR/css-text-3/). Passing a component's own fixtures does not replace
Breeze's pinned Web Platform Tests or end-to-end renderer tests.

## Candidate matrix

Upstream state was reviewed on 2026-08-24. Commit hashes make the maintenance snapshot explicit;
versioned documentation and releases remain the compatibility inputs for any future adoption.

| Candidate | Capability and standards surface | Fit with Breeze ownership | License, maintenance, and unsafe surface | Cost evidence | Decision |
|---|---|---|---|---|---|
| [Taffy 0.13.0](https://docs.rs/taffy/0.13.0/taffy/) | Typed CSS block, float, flex, and grid algorithms; high-level owned tree and low-level traits/functions. It does not supply inline text layout, tables, DOM, cascade, or painting. | Best candidate. The low-level API can eventually sit behind a Breeze-owned box tree and use the existing text measurer. The current single-pass layout/display-list coordinator is too early a seam. | MIT; Rust 1.71 minimum; release published 2026-08-08. Repository head `2c5715e6c7a12f54dfd839b2ac94f361af1a2ed4` was active on the review date. The 0.13.0 crate root denies unsafe code. | Selected `std,taffy_tree,flexbox` graph has Taffy, ArrayVec, and SlotMap. Official measured-content example built clean in 6.57 s and was 227,328 bytes. A matched Breeze consumer linked the adapter at +89,600 bytes (+2.48%). | **Adapt later**, starting with a box-tree/shadow-layout issue; do not adopt now. |
| [Stylo 0.20.0](https://github.com/servo/stylo/tree/v0.20.0) | Firefox/Servo CSS parsing, selectors, cascade, computed values, media queries, and invalidation. It is a style engine, not layout or painting. | Poor narrow fit. Breeze would implement Stylo DOM traits and replace `ComputedStyle`, property parsing, cascade, rule indexing, CSSOM serialization, and invalidation together. Keeping both style models would create a permanent translation layer. | MPL-2.0; release published 2026-08-04. Repository head `3c9c571d44d7b3d82e0b8981ddf5cc73661a68cb` was active on the review date. The published style crate contains about 123,552 Rust lines and a substantial shared-lock, traversal, generated-code, and unsafe surface that needs a dedicated audit. | No build probe: source/API review already crossed the time-box threshold for a narrow integration, and a partial build would not answer the DOM/property migration cost. Stylo already shares Breeze's `cssparser` 0.37 dependency, but that is a small fraction of the adoption boundary. | **Defer** until CSS coverage or style recomputation is the measured bottleneck. |
| [Blitz DOM 0.3.0-beta.1](https://crates.io/crates/blitz-dom/0.3.0-beta.1) | A beta HTML/CSS engine whose DOM integrates Stylo, Taffy, Parley, tables, inline layout, and optional AccessKit. | Useful integration reference, not a component boundary. `blitz-dom` would replace Breeze's renderer DOM, layout, text, accessibility-tree ownership, and invalidation. Its shell/network/rendering layers also overlap browser-owned services. | MIT OR Apache-2.0; Rust 1.89 minimum. Repository head `e0610f360eb0c8227321a4a8adc79114415b2795` was active on the review date. The published `blitz-dom` package is beta; current main follows an unreleased Taffy Git revision, demonstrating the maintenance coupling of the full stack. | No build probe: it would measure a second engine rather than an embeddable component. Published `blitz-dom` alone contains about 16,017 Rust lines before its Stylo, Taffy, Parley, rendering, and optional accessibility graph. | **Reject as a dependency**; adapt patterns and contribute upstream when relevant. |

Source-size counts above are local counts of Rust lines in the exact crates.io packages, included only
to show review surface. They are not code-quality or performance scores.

## Taffy adapter probe

Taffy was the one permitted prototype because it alone exposed a narrow layout API. The probe lived
under ignored `target/issue54-evidence`, used Taffy 0.13.0 with default features disabled, and linked
against Breeze's public DOM, `StyleSet`, and layout APIs. It did not change the browser binary.

The adapter:

1. parsed each HTML/CSS fixture through Breeze;
2. selected the flex container and children from Breeze's DOM;
3. translated the resolved `ComputedStyle` width, height, min/max size, flex basis/grow/shrink, gap,
   direction, alignment, and justification into Taffy styles;
4. computed a Taffy tree; and
5. compared Taffy's child rectangles with Breeze's painted solid rectangles.

| Focused case | Maximum rectangle difference | Result |
|---|---:|---|
| Two fixed 50 px items with `space-between` in 300 px | 0 px | Exact agreement |
| Two 50 px, `flex-grow: 1` items with a 10 px gap | 95 px | Both solvers place item 2 at x=155; Taffy sizes each item to 145 px while Breeze paints each specified box at 50 px |
| Two 150 px, `flex-shrink: 1` items in 200 px | 50 px | Both solvers place item 2 at x=100; Taffy sizes each item to 100 px while Breeze paints each specified box at 150 px |

The divergence is not evidence that Taffy is wrong. It exposed an existing Breeze boundary: flex
allocation controls the width passed through flow positioning, while block painting can still use the
child's specified width. Replacing only the allocation math would preserve that split and could make
geometry APIs, backgrounds, borders, hit testing, and display items disagree.

A production adapter therefore needs one authoritative box geometry per node, consumed by inline
measurement, child layout, painting, hit testing, CSSOM geometry, and presentation encoding. It also
needs cached Taffy node identity or Breeze implementations of the low-level tree traits; rebuilding a
parallel tree on every layout would forfeit incremental invalidation benefits.

### Build and binary probe

The isolated measurements used release mode on Windows x64 with Rust 1.95.0. They are sizing signals,
not promises for a production adapter.

| Measurement | Result |
|---|---:|
| Taffy official measured-content example, cold build | 6.57 s |
| Taffy example executable | 227,328 bytes |
| Matched Breeze-only consumer executable | 3,615,232 bytes |
| Same consumer plus parsed-style/Taffy adapter | 3,704,832 bytes |
| Linked adapter delta | 89,600 bytes (2.48%) |

The selected normal Taffy graph adds only `arrayvec` and `slotmap`. Grid would also add `smallvec`.
Compile and binary cost are acceptable; the blocker is ownership and compatibility, not package
weight.

## Headless compatibility and performance evidence

The public Wikipedia URL timed out at the hidden harness's hard 120-second process bound on the
review date, so it was excluded rather than reported as a performance sample. The checked-in
Wikipedia-like `encyclopedia-article` fixture is the reproducible representative workload.

Three release iterations ran through `benchmarks/run-alpha.ps1` with fresh hidden Breeze and unified
headless Chromium profiles. All three samples passed structural, visual, page-ready, and six-second
early-scroll acceptance:

| Metric | Median result |
|---|---:|
| Breeze page-ready | 202.1 ms |
| Chromium load-ready | 440.0 ms |
| Breeze layout/paint | 17.6 ms |
| Breeze scroll average / maximum | 2.8 / 3.5 ms |
| Breeze / Chromium working set | 49.1 / 579.2 MiB |
| Visual difference | 0.217, below the fixture ceiling of 0.28 |
| Scrolling layout rebuilds | 0 |

The pinned `CSS layout` WPT filter also passed 5/5 files and 6/6 harness subtests in hidden fresh
processes. It covers computed flex display/direction/grow and the currently gated CSSOM geometry
cases; it is not broad flex/grid conformance.

There is no candidate browser "after" number: the probe proved that routing a production path before
introducing authoritative box geometry would be invalid. The default browser graph and executable
were intentionally unchanged, so its before/after comparison is identity by construction. This is
preferable to publishing a benchmark from an adapter that cannot preserve painted geometry.

## Follow-up estimates and gates

### Taffy

1. **Box-tree and shadow-layout issue: 8-12 active AI-hours.** Define stable box identity and
   authoritative geometry, translate the currently implemented flex subset, run Taffy in comparison
   mode only, and add geometry-focused tests for grow, shrink, wrapping, auto margins, min/max size,
   replaced elements, and nested inline content.
2. **Flex production migration: 10-18 active AI-hours.** Route eligible flex containers through the
   adapter, preserve fallback for unsupported formatting contexts, expand pinned WPT coverage, and
   measure the complete alpha matrix. No permanent dual solver for the same container.
3. **Grid/block evaluation: 16-28 active AI-hours.** Only after flex is stable, evaluate grid and
   block/float migration separately. Tables and inline formatting remain Breeze-owned unless upstream
   capabilities and tests justify another explicit decision.

Adoption gates are no visual/geometry regression, no startup or scrolling regression beyond normal
run variance, no new browser-process authority, and a credible upstream path for missing behavior.

### Stylo

A future Stylo integration investigation is **20-35 active AI-hours** before adoption work: DOM trait
adapter, property-model gap analysis, cascade/CSSOM comparison, invalidation mapping, compile/memory
measurement, license obligations, and a substantially broader WPT slice. Production migration would
be a separate multi-stage project, not an alpha optimization.

### Blitz

No adoption estimate is warranted. Reviewing a specific Blitz integration pattern is **2-4 active
AI-hours** when a concrete Breeze boundary needs prior art. AccessKit remains independently scoped by
[#53](https://github.com/VictorZakharov/better-web-browser/issues/53); this decision neither blocks nor
implicitly adopts Blitz's accessibility ownership.
