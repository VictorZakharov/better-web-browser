# GPU Canvas presentation on Windows

Animated WebGL2 canvases can publish bounded shared D3D11 textures to the native browser instead of reading each frame into RGBA, converting it to BGRA, sending its pixels through renderer IPC and repainting it with GDI. A private shared EGL context resolves multisampled storage and produces top-down, premultiplied BGRA. The browser copies each admitted texture into an independently owned front, composes the page with Direct2D and presents through a DirectComposition flip swap chain. Browser chrome and native child controls retain their existing window ownership.

This is an internal presentation optimization. It does not change WebGL context attributes, Canvas dimensions, game quality settings, public bitmap export, `drawImage`, `readPixels`, query publication or drawing-buffer retirement. There are no site-specific URLs or adaptive quality reductions in the implementation.

## Ownership and ordering

The existing WebGL owner thread owns all EGL contexts and surfaces. Author commands flush before presentation. The copy context shares texture and renderbuffer storage while keeping independent framebuffer bindings, queries, transform feedback and draw state. Both the MSAA resolve and the alpha/orientation conversion run there; resolving in the author context can affect an active native occlusion query on the admitted ANGLE provider. The native owner completes the copy before external D3D use, as required by the ANGLE surface extensions; this synchronization never runs on the HWND thread.

Each Canvas pool has three producer-owned slots. The latest completed slot remains intact for a later CPU repair. Every rewrite advances its nonzero keyed-mutex generation; an older descriptor cannot acquire a rewritten slot. A producer may reclaim a published but unconsumed generation, allowing renderer presentation coalescing without an acknowledgement queue. The browser polls the requested key with zero timeout, copies the frame into its own texture, flushes and releases key zero. Subsequent HUD-only paints reuse that front without reacquiring old metadata.

Only `S_OK` from `AcquireSync` grants ownership. `WAIT_TIMEOUT` is a retry, and `WAIT_ABANDONED` is a failure even though both are positive HRESULTs. EGL owns the legacy DXGI share handle: it is never passed to `CloseHandle` or treated as an NT kernel handle.

Renderer protocol version 18 carries a complete document-scoped set of GPU Canvas descriptors alongside the existing presentation resource deltas. It contains integer identities, extents, adapter LUIDs and legacy share-handle values, with no process pointers or pixel buffers. Admission validates unique Canvas URLs and surface identities, dimensions, budgets, matching retained image extents, the exact native texture descriptor and the selected hardware adapter. Old-document messages remain subject to the existing navigation and presentation revision checks.

## Bounds and fallback

| Resource | Bound |
| --- | --- |
| Producer slots | Three per context; 64 MiB total per renderer owner |
| Individual Canvas | 4096 per dimension; at most 4,194,304 pixels |
| Presentation set | 64 canvases; 64 MiB of logical fronts |
| Browser imported surfaces | 64 MiB and 192 entries |
| Browser fronts | 64 MiB and 64 canvases |
| Native bitmap cache | 64 MiB and 4096 entries |
| Native composition target | 64 MiB |
| Paint groups | Depth 64; 64 MiB of active layer bounds |
| Queued native submissions | At most two incomplete completion fences |
| Handoff retry | Zero-timeout polls; CPU fallback after two seconds |

The GPU slot quota is separate from author-visible WebGL allocation accounting. Presentation admission must not change whether an author's texture or buffer allocation succeeds. A failed export is remembered for its current extent to avoid repeating unsupported allocations every frame; a resized extent can be admitted independently.

Initial and resized Canvas bitmaps use the existing CPU path to establish replaced-element geometry. GPU presentation currently admits untinted, axis-aligned Canvas placement, rectangular clips, bounded opacity/mask groups, renderer glyphs, retained images and supported page primitives. Multicolored borders share the native painter's proportional corner regions, including transparent sides. Rotated Canvas placement/clips, unsupported native control content, excessive working sets, unavailable adapters, invalid metadata and device failures decline GPU composition for the document. The ordinary CPU renderer remains available for WebGL1 and other backends.

When composition declines an unpreserved Canvas, the renderer recovers its retained exported frame rather than reading an already cleared drawing buffer. Disabling the policy requests an immediate repair checkpoint, even for a static Canvas with no future author callback. A temporarily held slot is retried; permanent ownership/readback failures stop retrying and report a diagnostic. Tab suspension retires the HWND compositor, disables the old document's policy and waits for its CPU repair before GPU admission resumes. Reader mode and crashed content cannot retain a page composition visual over the native surface.

Ordinary UI composition never waits for a completion fence, maps GPU pixels or waits for vertical refresh. It runs at the existing bounded paint opportunity after renderer installation and ready input have had their turn. A retry timer requests paint only; it does not advance the document clock. Explicit benchmark screenshots may wait briefly for completion and read back the owned composition target.

Image bounds retain the native painter's independent rounding rule, while clip bounds retain its enclosing rule. Glyph rasters already have final physical DPI and are drawn at natural dimensions. Rounded native shapes reuse cached GDI region coverage. Opacity and mask composition uses Direct2D premultiplied blending; small integer-rounding differences from the retained GDI path are expected and are measured in pixel comparisons.

## Validation and measurement

Focused contracts cover metadata validation, generation reuse, consumer backpressure, alpha conversion, orientation, MSAA, active author queries/transform feedback and recovery after unpreserved-buffer retirement. The opt-in renderer-process test crosses the real AppContainer boundary, verifies every pixel of an asymmetric frame and requests CPU repair without advancing the author clock. Run it as the normal Windows user:

```powershell
cargo test --test renderer_process contained_gpu_canvas_texture_handoff_preserves_pixels -- --ignored --nocapture
```

The mixed Canvas/DOM fixture exercises opaque, premultiplied and straight-alpha input, clipped scaling, nested opacity, masks, MSAA and retained HUD content. The existing frame-publication fixture checks 65 changing frames and public GL pixels. Run browser comparisons through `scripts/run-hidden-benchmark.ps1`, sequentially, with matching dimensions and DPI. Game functionality checks keep High quality selected; their loading time is not an isolated performance workload.

The opt-in completed-command test compares CPU readback/conversion/upload with shared-frame export/import/composition on the same hardware adapter. Authored drawing completes before timing; both routes include completion of native composition. Three warmups precede each sample block, candidate/reference order rotates and every sample verifies completed pixels outside the timed region. Preserve individual samples and ranges; other agents can still affect scheduling.

```powershell
cargo test --release --lib gpu_canvas_completed_command_samples -- --ignored --nocapture
```

This command excludes JavaScript/V8, renderer IPC, GDI page painting and DWM/window presentation. The hidden browser's `gpu_composition.last_submit_ms` measures submission only, not GPU completion or gameplay FPS. Once GPU content is active, the existing GDI native-paint trace covers chrome/CPU painting and cannot represent total page composition cost.

The October 13, 2026 release comparison retained 36 samples per route at each extent, in three alternating blocks of 12. No builds or heavy suites from this task overlapped the measurement, and no samples were discarded. Other agents on the shared workstation can still affect scheduling. The following times include completed commands and have verified output pixels; they are not a gameplay FPS prediction or isolated CPU cost.

| Extent | CPU median (range), ms | GPU median (range), ms |
| --- | --- | --- |
| 128 × 128 | 0.246 (0.236–0.431) | 0.268 (0.261–0.512) |
| 1382 × 760 | 3.274 (3.149–3.577) | 0.282 (0.272–0.678) |
| 2048 × 1024 | 6.309 (5.969–7.804) | 0.320 (0.304–0.558) |

The small Canvas workload adds about 0.022 ms at the median. The larger workloads benefit from avoiding CPU readback, conversion and upload. The 65-frame publication fixture matched the retained renderer's capture exactly. The mixed composition fixture had 33 pixels differing by more than one channel value, with a maximum difference of two, at opacity/mask edges. The patterned lifecycle fixture also matched exactly in the content area; resize, detach/reattach, context restoration, tab switching and Reader mode retained working presentation, while rotated placement exercised CPU recovery.

The final local verification passed all-target/all-feature Clippy, 6,676 non-ignored tests, the opt-in hardware sharing and real AppContainer handoff tests, 583 WPT pages with 6,425 subtests, and 74 Khronos pages with 8,696 subtests. Browser functionality and capture checks run separately from the command timing workload.

## Primary references

- [EGL 1.5 specification](https://registry.khronos.org/EGL/specs/eglspec.1.5.pdf), context sharing in section 3.7.1.
- [ANGLE shared D3D texture surfaces](https://registry.khronos.org/EGL/extensions/ANGLE/EGL_ANGLE_surface_d3d_texture_2d_share_handle.txt) and [ANGLE keyed mutex extension](https://raw.githubusercontent.com/google/angle/main/extensions/EGL_ANGLE_keyed_mutex.txt).
- [DXGI keyed-mutex ownership results](https://learn.microsoft.com/windows/win32/api/dxgi/nf-dxgi-idxgikeyedmutex-acquiresync).
- [D3D11 nonblocking completion queries](https://learn.microsoft.com/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-getdata).
- [Direct2D and Direct3D interoperability](https://learn.microsoft.com/windows/win32/direct2d/direct2d-and-direct3d-interoperation-overview).
- [DXGI flip presentation](https://learn.microsoft.com/windows/win32/direct3ddxgi/dxgi-flip-model).
- [Native region coverage data](https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-getregiondata).
