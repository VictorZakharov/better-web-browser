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
