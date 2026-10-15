# Retained HUD updates

Issue [#244](https://github.com/VictorZakharov/better-web-browser/issues/244) tracks gameplay frame stalls. Two independently reproducible rendering costs are addressed here: reconstructing boxes for opacity samples, and reconstructing unrelated page content when a viewport-fixed HUD replaces text or updates an SVG graph. The unchanged application remains a separate functionality check; a synthetic workload does not establish a game FPS improvement.

## Compatibility contracts

Opacity updates reuse an existing, renderer-owned group only when every changed computed style differs exclusively in opacity, both values remain below one, and generated paint is unchanged. Exact comparison rejects future properties automatically. The update validates all affected owners before mutating paint. Inline fragments, unknown owners, resource/viewport changes and pending frame layout use the normal path. Crossing opacity one uses normal painting because it changes stacking-context participation; see [CSS Color 4, transparency](https://www.w3.org/TR/css-color-4/#transparency).

Fixed-panel reflow uses the existing anchored viewport-fixed ownership proof. Proven text-child replacement and known graphics attributes may change text identities, but the panel's DOM element membership must remain identical. Removed identities must belong to the refreshed panels, and their obsolete fragments are removed before the new subtree is installed. Multiple panels are validated before any replacement. Existing limits on members, images, native controls, sticky layers and top-layer content remain in force.

Text-sensitive or graphics-sensitive nonlocal selectors, scoped/shadow rules, arbitrary attributes and element insertion keep the full-layout fallback. Changes to an unrelated image's natural size also invalidate reuse. These restrictions preserve selectors, stacking, clipping, normal-flow geometry, hit testing and resource ownership. Viewport-fixed boxes have independent containing blocks under [CSS Position 3](https://www.w3.org/TR/css-position-3/#fixed-pos); transformed/contained ancestors and automatic insets still require the existing conservative admission checks.

## Reproduce a comparison

Save the reference executable before rebuilding. Use canonical release builds for performance claims, and finish this agent's builds and heavy suites before collecting samples. Run from the repository root as the normal Windows user:

```powershell
./scripts/measure-hud-updates.ps1 -Reference G:\bench\reference.exe -Candidate G:\bench\candidate.exe -OutputDirectory G:\bench\hud-results
```

The default comparison runs three rounds per workload in reference/candidate, candidate/reference, reference/candidate order. It starts a hidden loopback fixture server and uses the common guarded hidden browser launcher. Each sample verifies a real Canvas marker and a blended opacity patch in the native screenshot. Reports, logs, screenshots, executable hashes and an exact fixture copy/hash are saved on G:. No busy-machine outlier is removed.

| Workload | Animation | HUD text and SVG updates |
| --- | --- | --- |
| `opacity` | Eight alternating opacity samples | Text every 250 ms |
| `content` | Disabled | Eighteen counters and an SVG path every frame |

The original workload contains approximately 1,200 unrelated card elements, independent fixed panels and a Canvas FPS graph. Its movement phase dispatches synthetic document mouse events; it does not simulate game physics. Optional `geometry=1` adds a synchronous rectangle read per mouse event; default samples check geometry at phase boundaries. Reported FPS and frame intervals include scheduling and contention, and are not isolated CPU costs.

## Held movement in hidden game checks

`run-hidden-benchmark.ps1 -ActionSequence` also accepts `keydown:w,KeyW`, `pause:1000` and `keyup:w,KeyW`. They dispatch document-owned trusted key phases through the existing renderer input path; they do not send OS keyboard input. A plain `key:w,KeyW` still presses and releases immediately. These options require benchmark mode.

The fixture reports trusted key phases and the number of animation frames observed while W was held, so a held-input smoke check can distinguish actual movement input from isolated key taps. Keep native game checks separate from the small timing comparison, and record the application build, biome, quality, viewport and device scale. Opt-in checkpoint diagnostics identify dirty roots, the first changed style owner, and whether independent fixed layers or opacity groups were reused.
