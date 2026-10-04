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
   animation scheduling and compositor presentation. Current WARP software
   rendering and 64 MiB per-context budget are baseline containment policies,
   not evidence of acceptable gameplay performance. Change limits only with
   accounting, pressure behavior and containment tests.
5. **Application integration.** Exercise module loading, image/Canvas texture
   sources, input/pointer lock, audio, persistence and workers. The cape worker
   pool transfers typed-array buffers; its name alone is not proof that it
   requires worker-side GPU rendering. Multiplayer uses `trystero` and must be
   assessed separately from single-player rendering.

## Acceptance and verification

The current batching preference is approximately **20,000 useful added lines**,
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
