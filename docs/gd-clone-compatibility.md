# Long-term 3D acceptance target: gd-clone

The sibling `gd-clone` project (Last Stand) is a real application target for
Breeze's standards work. This is a dependency roadmap, not a claim that the game
currently runs. The inspection on 2026-10-03 found Three.js **0.185.1** and an
explicit `webgl2` check in `src/ui/loading.ts`. Do not downgrade its library,
replace its renderer, spoof WebGL 2, or change the game to conceal browser gaps.
The inspected sibling HEAD was `9669c02b2db12630dae184f684486a996c3e32bf`;
requirements should be rechecked when that application changes.

## Implementation order

1. **Texture and render-target foundations.** Bounded float/half-float uploads,
   filtering, color-renderable attachments, depth textures and sRGB behavior.
   WebGL 1 extensions are useful independently, but do not satisfy the game's
   WebGL 2 requirement. Its `src/core/renderer.ts` uses half-float targets for
   the scene, bloom, sanitization, grading and three downsampling stages.
2. **Real WebGL 2.** Create an actual GLES 3 context through the existing ANGLE
   backend, implement the WebGL 2 shader/API validation boundary, sized texture
   formats, vertex arrays, instancing and the state/query operations used by
   modern Three.js. Expose the context only once the advertised contract works.
3. **HDR, shadows and multisample resolve.** The game's scene target requests
   four samples; its post-processing targets are single-sample. Implement
   separate read/draw framebuffers, multisample storage and framebuffer blits,
   depth/shadow sampling and color-buffer-float capability gating. Its shaders
   use `isnan` and `isinf`, requiring the correct GLSL ES 3 compiler path.
4. **Sustained rendering.** Evaluate hardware ANGLE adapters, safe resource
   budgets for a real scene and post-processing chain, batching/transport cost,
   animation scheduling and compositor presentation. Hardware-first ANGLE
   admission now has WARP fallback, but the 64 MiB per-context budget and current
   transport/compositor policies are baseline containment policies,
   not evidence of acceptable gameplay performance. Change limits only with
   accounting, pressure behavior and containment tests.
5. **Application integration.** Exercise module loading, image/Canvas texture
   sources, input/pointer lock, audio, persistence and workers. The cape worker
   pool transfers typed-array buffers; its name alone is not proof that it
   requires worker-side GPU rendering. Multiplayer uses `trystero` and must be
   assessed separately from single-player rendering.

## Acceptance and verification

The October 4 batch admits real public WebGL2 contexts and includes an optional
fixture importing unchanged Three.js 0.185.1. Its twelve scenes cover indexed
textures, instancing, volume/array sampling, shadows, physical materials, morph
targets, HDR multisample resolve/post-processing and resize. These are bounded
pixel contracts, not an acceptance run of the game's production build. See
[the native contract](webgl2-foundations.md) for provider limitations and replay
instructions. Sustained gameplay and application integration remain next steps.

The current batching preference is approximately **15,000 useful added lines**,
including focused tests and documentation, with separate reviewable commits for
the constituent standards slices. Do not pad changes to meet the target. At the
daily 9 p.m. America/Toronto boundary, publish a draft PR for completed work even
if the target has not been reached; identify incomplete scope and outstanding
verification explicitly. The cutoff includes builds and handoff, not just edits.

For each slice, require native pixel tests, invalid-input/resource-isolation
tests, and shared fixtures against unified-headless Chromium. Use compatible
upstream implementations where available; ANGLE already supplies native GLES
and shader translation. Retain licensing and provenance checks for additions.

Progress towards the game should eventually be measured with its unchanged
production build: lobby rendering, starting a single-player run, movement,
combat, shadows, bloom and resize, followed by sustained frame-time and memory
measurements against Chromium on the same machine and graphics adapter. A
successful context creation or increased HTML5test score is not that milestone.
Automated application checks must remain hidden, and generated builds/profiles
must stay on G:. Do not modify or run the sibling's build scripts as part of a
read-only requirements audit.

### October 5 startup investigation

The existing production build was copied without rebuilding or modifying the
sibling, at HEAD `c36cbb223a1a0ccd54e7b1e7e3716e2e4e9522cd`. The copied asset set
(`index-FyQsDf7S.js`, `index-CfGsp_J8.css`, unchanged Three.js 0.185.1) was held
constant for hidden release-browser comparisons on this machine. Each run used
a fresh profile and an eight-second settling period. These are single-run
diagnostics, not a statistically stable gameplay benchmark.

| Measurement | Preceding release | SVG input-cache fix |
| --- | ---: | ---: |
| First presentation | 1,834 ms | 831 ms |
| Cumulative style/resource refresh | 9,331 ms | 1,151 ms |
| Executed scripts | 1 | 2 |

Animation sampling advances DOM mutation generations even when an SVG's
serialized decoder input is unchanged. Raster-cache keys now use that actual
input, including resolved `currentColor`, rather than treating every sample as
a new drawing. This reuses the existing resvg decoder and does not bypass real
attribute, text or inherited-color changes. Tests verify pixel allocation reuse
for opacity/transform samples and replacement after a fill change.

The module now executes, but startup still fails: texture-generation work in
the document and a worker reaches the two-second JavaScript execution limit.
The loading screen also has unresolved layout/SVG-definition differences. The
Chromium reference reached "Lighting the lobby" in its earlier eight-second
capture; neither capture proves sustained gameplay. Watchdog containment,
worker scheduling and the remaining visual differences need investigation
before claiming the unchanged game runs.

The subsequent optimized-iteration build, with native clone-byte conversion and
Canvas row copies, no longer reported a worker timeout in the eight-second
capture. It still reported a document timeout. A temporary instrumented copy
identified a 512-square Canvas `stroke()` as the active call when interrupted;
the instrumentation was removed after diagnosis and is not part of the game or
browser. Native stroke rasterization is the next investigation, not a reason
to raise or disable the watchdog. Final performance claims require fresh
release-mode measurements after the batch is complete.

### October 5 wrap-up release capture

The final batch replay uses that same unchanged production asset snapshot, a
fresh profile, 125% device scale and twenty seconds of settling. The hidden
release browser recorded first presentation at 897.520 ms and cumulative
JavaScript time of 6,748.820 ms. It still reported a document timer promise-job
execution-limit exception after 2,000 ms. A textured background, tip and loading
shape were visible, but no accepted lobby/gameplay transition occurred. These
single-run numbers do not establish sustained frame times or a startup-speed
comparison with Chrome. Native adapter HLSL precision warnings were also
recorded; they are not treated as proof of the JavaScript failure's cause.

The completed work preserves the watchdog and adds generally useful native
coverage/shading, clone transport, shader validation reuse and real multi-draw
and indexed-blending contracts. Dedicated fixtures exercise actual pixels and
invalid updates in Window and Worker, rather than relying on capability flags.
The next game investigation should profile the remaining document texture
generation and loading-screen differences against the unmodified reference.

SVG text is not included in this batch: the pinned usvg layout does not correctly
apply an ancestor's `textLength` across nested tspans. Its failing regression and
prototype remain in ignored G: scratch files for follow-up; no test was weakened
or ignored to ship it. Existing SVG raster/image/input-cache improvements remain
included. The HTML5test release score is unchanged at 507 / 588 in the fresh
before/after samples, versus 579 / 588 in the hidden Chrome reference.

### October 6 owned Canvas/font batch

The final hidden release replay uses the same unchanged production snapshot,
a fresh profile, 125% device scale and twenty seconds of settling. Page readiness
was 861.332 ms and cumulative JavaScript time was 5,944.950 ms. A document timer
promise job still exceeded the unchanged 2,000 ms execution limit. The capture
shows the textured loading background and tip, with a large loading shape and
remaining layout differences; it does not show an accepted lobby or gameplay.
These single-run timings are not a sustained-performance or Chrome comparison.

Dedicated readback-fenced Canvas measurements improved repeated text, image
painting and compositing substantially, while Chrome remains faster on the
layer workload. Real CSS font sources, OpenType features and private Canvas
ownership now have unit, renderer and upstream WPT coverage. The game blocker
remains a separate document texture-generation/watchdog investigation. No game
asset, watchdog limit or containment budget was changed to obtain this result.

### October 6 font-readiness / SVG-text wrap-up

The final release replay uses the same unchanged production asset snapshot,
fresh profiles, twenty seconds of settling, 100% device scale and a measured
1262-by-539 CSS-pixel viewport in both Breeze versions and Chrome 154.0.8037.98.
Chrome reaches the rendered lobby. Breeze renders the real SVG title over the
loading background, but its title/layout is too large and no accepted lobby or
gameplay transition occurs. Both Breeze versions report a document timer
promise-job execution-limit exception at the unchanged 2,000 ms watchdog.

| Single-run diagnostic | Before (`e349b7e`) | After | Chrome |
| --- | ---: | ---: | ---: |
| Harness page-ready / first presentation | 893.495 ms | 2,780.700 ms | 994.889 ms |
| Cumulative JavaScript time | 6,739.688 ms | 5,347.702 ms | 7,290.274 ms |
| Reported working set | 429.7 MiB | 470.4 MiB | 1,227.9 MiB |
| Accepted captured state | Loading only | Loading only | Rendered lobby |

**Breeze first presentation regressed in this sample.** Lower cumulative script
time is not a game-startup win: the browsers finish different amounts of work.
Chrome's page-ready observer is not Breeze's renderer-presentation observer,
and its lobby workload differs from Breeze's interrupted loading workload.
These single runs do not establish a browser-speed or memory-efficiency ranking,
sustained frame cadence, time-to-lobby, or playability. They are a transparent
remaining blocker, not a reason to raise the watchdog or modify the game.

### October 7 responsive sizing and CPU attribution

The game's `min(620px, 88vw)` title width now uses a general deferred CSS
comparison expression. A separate controlled fixture matches Chrome for nine
used rectangles at CSS viewport widths 1,262, 500 and 320, within 0.02 CSS pixels
for x/y/width/height. Empty automatic-height aspect-ratio boxes no longer let
their margins collapse through their ratio-dependent height. This fixes both
the SVG size and the following sibling's flow; it is not a game stylesheet patch.
See [the CSS contract](css-comparison-lengths.md).

The same unchanged production assets still do **not** reach accepted lobby or
gameplay in Breeze. Fresh diagnostics retain the two-second worker/document
execution-limit failures. An actual failed worker task used approximately
1.9 seconds of owner-thread CPU inside its approximately two-second elapsed
interval. GC callback spans ranged from tens of milliseconds, not most of that
budget. These samples suggest substantial computation, not solely scheduling
delay or collection pauses; they do not identify the engine-build cause.

The production worker error handler falls back to main-thread texture generation
for missing recipes. That is a plausible contributor to subsequent document
failures, not proof of the cause of every timer exception. No timeout, worker
pool limit, texture resolution or game asset has been changed. Remaining loading
shape/gradient differences are also not accepted visual parity.

Controlled Canvas region measurements improved the warmed median from 18.8 to
10.5 ms, with identical Breeze before/after pixels. Chrome's 1.2 ms median remains
much faster on that specific CPU/readback fixture. Numerical worker fixtures did
not show a reliable improvement from ordinary WorkerGlobalScope setup, so that
realm correction is not reported as a game-startup speedup. See
[ownership, measurements and diagnostic limits](canvas-region-ownership.md).

The preceding single-run first-presentation regression was not consistently
reproduced: repeated fresh captures varied with resource readiness and completed
work. Current approximately 1.8–2.0-second presentations still correspond to
interrupted loading, not time-to-lobby. Neither the lower Canvas cost nor a
different first-presentation sample establishes that the game-loading blocker
is fixed.

The [local Windows V8 build experiment](v8-build-experiment.md) did not remove
the document watchdog failure or reach the lobby in any of three fresh runs.
Its archive and generated bindings were paired and tested in an isolated target;
the normal shipped backend remains unchanged. The matched normal-build median
first-presentation sample is still slower than merged #228, so that investigation
also remains open.

The subsequent clipped-shader extension preserves the tested Breeze pixels and
reduces its linear/radial/pattern readback batch medians to 1.6/2.0/1.9 ms.
That independent Canvas improvement does not remove the game blocker: three
fresh captures still retain loading and document execution-limit failures,
while matched Chrome captures reach the lobby. The normal median harness
page-ready measurement is 2,591.0 ms versus 2,004.5 ms for merged #228 in this
replay; both resource timing and interrupted work vary, so no first-presentation
fix or time-to-lobby claim follows.

A separate diagnostic replays all 21 recipes from the frozen production worker
sequentially without modifying their algorithms or output sizes. Both browsers
complete every recipe. Breeze's three face computations take about
1,283/842/1,357 ms, versus Chrome's 1,132/748/1,144 ms; its forest-floor/cobblestone
computations take 849/664 ms versus 795/622 ms. These are single diagnostic
samples, not stable speed rankings or proof that four concurrent workers meet
the same deadline. They rule out a universally nonworking recipe and do not
justify shipping the experimental V8 backend or increasing the watchdog.

The subsequent bounded document-task capture attributes the failed promise job
to 2,012.3 ms elapsed and 1,968.8 ms owner-thread CPU, with 30 collections and
only 11.8 ms in GC callback spans. In that same capture the two failed texture
worker tasks use about 2,015.6 ms owner-thread CPU each, versus 17.1–24.6 ms
in GC callback spans. These are individual diagnostic observations, not median
performance results. They narrow the investigation toward executed work rather
than a predominantly idle/network or garbage-collection delay. Detailed
[opt-in CPU source sampling](cpu-task-sampling.md) is separately gated and its
overhead must not be confused with an unprofiled acceptance result.

The later ordinary acceptance replay removes Breeze's diagnostic selectors as
well as the CPU sampling flag. Three fresh runs per browser use the unchanged
production snapshot, a 1262-by-539 CSS-pixel viewport and 30 seconds of settling.
Every before/after Breeze run still retains loading and a document execution-limit
failure; the game blocker is therefore not merely a profiler artifact.

| Ordinary acceptance replay | Merged #228 | Current normal backend | Chrome 154 |
| --- | ---: | ---: | ---: |
| Median harness page-ready observation | 1,822.5 ms | 1,895.7 ms | 1,175.5 ms |
| Individual observations | 1,837.7 / 1,737.3 / 1,822.5 ms | 1,895.7 / 2,044.2 / 1,793.0 ms | 1,517.3 / 813.3 / 1,175.5 ms |
| Breeze loading/document failure | 3/3 | 3/3 | Not applicable |

These remain distinct harness milestones, not matched time-to-lobby measurements.
The timing spread and different completed work do not establish a browser ranking.
Sampled runs sometimes suppress the reported timeout but still stop at
“Conjuring foes and spells”; that is not a successful startup result either.
Source samples in completed document tasks identify private Canvas serialization
and ImageData row copying among executed work. Worker samples identify the
unchanged procedural texture functions. Statistical source hits are not exact
CPU durations, and unresolved/native hits must not be assigned to JavaScript.

The later packed-geometry / contiguous ImageData / private WebGL-wire release
still fails ordinary startup acceptance. A fresh unprofiled 30-second capture
reports the document's 2,000 ms promise-job watchdog and retains “Conjuring foes
and spells”. It reports no worker timeout in that particular run, which is not
enough to establish reliable worker completion. Separately enabling source
sampling removes the reported error in one capture but still leaves the loading
overlay and hidden menu. The sampled run is therefore not a performance or
startup success result. The watchdog, worker recipes and shipped V8 archive are
unchanged.

The subsequent source-category diagnostic keeps ordinary startup acceptance
separate from sampling. A fresh ordinary 30-second replay still retains the
“Conjuring foes and spells” loading overlay and the document promise-job watchdog;
its 2,087.15 ms harness page-ready observation is not time-to-lobby. In the
separate sampled replay, two completed slow document tasks contain 34/46 and
45/57 native-callback hits, versus 1/46 and 2/57 VM-internal hits. The reported
Canvas solid-path calls total 610.692 ms in one task, while context creation takes
200.071 ms in another. These individual, instrumented observations do not prove
the failing ordinary task has the same profile. They do argue against treating
all previously unresolved non-script hits as V8 compiler work. No new engine
backend or enlarged execution limit follows from them.
