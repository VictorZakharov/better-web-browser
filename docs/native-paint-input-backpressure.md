# Native paint and input backpressure

This batch addresses measured native image commands and a dropped-key-release failure. It does not change the game's source, quality, assets, WebGL commands, logical animation interval or memory defaults. Loading and title-particle improvements from the previous batch remain intact. These changes add no web API surface and make no HTML5test score claim.

## Accepted presses own release capacity

The ordinary browser-owned deferred input queue remains eight events. Previously, a keyboard press accepted by the renderer could be followed by a rejected release when those eight slots were occupied. A document then had no event that could clear its movement state. An accepted physical press now reserves capacity for one eventual release, independently of the ordinary queue. Both deferred presses and directly admitted presses acquire this ownership; an unaccepted press does not.

The reservation key is the document plus physical `code`, not shifted `key`, focus target or modifiers. Distinct unidentified native virtual keys use their fallback key names. Typematic repetition does not allocate more reservations. Up to 256 held presses plus queued owed releases share a finite budget; a new press beyond that budget is rejected with an actionable status. A queued release retains its slot through a failed broker retry and releases it only on successful admission or document retirement. Navigation clears the document's ownership.

Retained events preserve their FIFO position and discrete barriers. The fix neither moves release events ahead of accepted presses/clicks nor drops intermediate keyboard events. Existing ordinary pointer-move acknowledgment and coalescing contracts remain unchanged. Queue saturation, repeat barriers, shifted keys, unidentified keys, navigation and retry/capacity accounting have unit tests. This addresses the concrete missing-release bug; it does not promise zero latency during an arbitrarily long author task or implement synthetic releases on focus loss. See [UI Events keyup](https://www.w3.org/TR/uievents/#event-type-keyup).

## Native image commands

One paint-owned source DC is reused for glyph, image and background bitmap copies. Each draw restores the previous selected bitmap before cache retirement; the source DC is deleted after the traversal. A null reusable DC retains the existing independently owned fallback. Opaque copies also restore the destination stretch mode. Pixel tests exercise translucent and opaque sources, fractional scale, reflection, clipping and native object selection.

Image, solid and bounded mask/opacity commands reject only rectangles excluded by the actual native DC clip, using [RectVisible](https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-rectvisible). This includes transformed and rounded overflow clips and dirty-region backbuffer offsets. Partially visible draws keep their original destination rectangle, not a cropped rectangle that changes resampling. Excluded nested groups preserve balanced boundaries without allocating a layer; transform groups alone are not culled from an assumed local bounding box.

Repeated horizontal reflections can share the existing byte-bounded scaled-image cache. Admission captures the exact native float matrix, local rectangle and resulting device-grid placement. The first request paints directly; a second identical request bakes the original [AlphaBlend](https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-alphablend) operation into a zeroed premultiplied DIB and replays it at integer device coordinates. The selected clip and viewport offset remain owned by the destination DC. No guessed pre-flip or alternate sampler is used.

This optimization is deliberately limited to horizontal reflection with positive vertical scale, ordinary text mapping, zero window origin and bounded coordinates. Vertical reflection, shear, rotation, RTL mapping, unsupported mapping and extreme coordinates keep the direct painter. Vertical baking did not pass the pixel comparison and is not shipped. Source pixel updates and removal invalidate every associated normal/reflected variant, including same-size DIB reuse. Reflected variants share the existing 64 MiB, 512-entry, 1,024-admission-history and 4 KiB-key bounds; they do not create a second memory allowance.

## Paint opportunities outside installation

First presentation retains its existing synchronous paint contract. Subsequent presentations invalidate native damage and request one window-owned paint opportunity. The message pump services renderer/runtime work after the window procedure returns, then paints outside the installation borrow. Ready native input gets an opportunity first, but repeated damage cannot restart an eight-millisecond paint fairness deadline. An ordinary WM_PAINT consumes the same pending opportunity. No window messages are discarded, no private timer spin is added and no animation callback deadline is rewritten. [UpdateWindow](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-updatewindow) dispatches WM_PAINT synchronously; avoiding that re-entry during every installation is the ownership boundary, not a new frame-rate claim.

## Reproducible verification

Run native tests with `cargo test --release --locked --bin better-web-browser -- --test-threads=1`. Two ignored tests isolate completed commands without opening a window: `native_clipped_sprite_command_samples` and `native_horizontal_reflection_command_samples`. Run each with `-- --ignored --nocapture --test-threads=1`, sequentially and without this agent's builds or browser suites overlapping. Four rotated reference/candidate rounds retain 12 individual samples per route per round. Surface allocation, cache warming and exact output checks are outside the clock; timed native drawing includes source-DC setup where needed and GdiFlush. Preserve all samples and scheduling outliers on G: rather than interpreting wall time as isolated CPU cost.

[The generic image fixture](../benchmarks/ui/fixtures/native-clipped-images.html) contains 354 original inline SVG sprites, two rounded overflow clips, a horizontal reflection, an excluded nested mask/transform and a bounded changing Canvas. Its final output and logical geometry are checked in both Breeze and headless Chrome. Captures use the guarded hidden benchmark launcher, one browser worker and fresh G: profiles. At a 1,536 by 864 CSS-pixel viewport and DPR 1.25, candidate and pre-batch Breeze content pixels match exactly. Chrome has the same geometry and visible sprite arrangement; existing native raster/edge differences mean this is not a claim of byte equality with Chrome.

Release samples on this shared Windows workstation (2026-10-11; 48 samples per route, rotated sequential order, exact output verified):

| Completed command | Reference median (range), ms | Candidate median (range), ms |
| --- | --- | --- |
| 354 translucent sprites, unclipped | 4.463 (4.250–5.772) | 1.670 (1.650–1.854) |
| Same sprites, 240×140 clip | 4.194 (4.021–4.790) | 0.388 (0.375–0.446) |
| 32 repeated horizontal reflections, native backbuffer | 8.378 (7.887–9.672) | 3.595 (3.454–5.380) |

The sprite command combines source-DC reuse and native clip rejection. The reflection command isolates the warmed cache against direct native reflection, including the original device-bitmap destination type. A preceding small heavily clipped DIB experiment did not establish a reflection-cache benefit; the retained optimization targets repeated native-device sampling, not every reflected draw. All individual release and exploratory samples remain in `target/frame-input-latency-20261011/` on G:, outside commits. These are command costs, not frames per second.

The unchanged Last Stand page is a separate functionality check, not the command timing baseline. Fresh hidden captures of both the pre-batch and candidate browsers can currently reach a live lobby with a black arena; those captures cannot establish gameplay correctness or visible FPS improvement. The user's latest 9.5 FPS average and occasional 4 FPS reports remain the gameplay evidence until a visible test of the final release build provides new data. Do not substitute hidden callback cadence, NativePaintTrace timings or command speedups for gameplay FPS.

Full local renderer verification exposed an existing worker-stream cancellation fixture race, reproduced against both executable versions. Its helper advances virtual time while waiting for worker startup, so the fixture's termination timer could fire before its active-stream case began. The fixture now triggers termination through explicit keyboard input after observing the pending entry or response headers, retaining the original fetch-abort, blocked-producer cancellation and surviving-renderer assertions. The timeout was not extended and production worker behavior was not changed; temporary phase logs and executable overrides are not committed.
