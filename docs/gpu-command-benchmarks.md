# Small GPU command comparisons

Optimize repeated slow operations using a fixed, pixel-checked workload rather
than treating a live game's wall-clock startup as a command benchmark. Network,
procedural generation, caches and other desktop agents can dominate that clock.
Keep the unchanged game as a separate loading/rendering correctness check.

`benchmarks/gpu/fixtures/numeric-setters.html` measures `clearColor`,
`uniformMatrix4fv` and `uniform4fv`, with both constant and changing inputs.
Each phase warms up 1,000 calls, then performs 100,000 calls in twenty bounded
5,000-call tasks. Active time includes `getError` draining the ordered command
queue, not actual GPU completion. Inter-task timer waits are excluded. The final
shader must really draw the expected green pixel with no GL errors.

This is API submission plus an ordered error-query boundary, not isolated CPU
time. Chromium's [WebGL binding](https://chromium.googlesource.com/chromium/src/+/HEAD/third_party/blink/renderer/modules/webgl/webgl_rendering_context_base.cc)
delegates a clean-context error query to its GL client; that client's
[GetGLError implementation](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/gpu/command_buffer/client/gles2_implementation.cc)
queues the query and waits for its command result. Neither that wait nor a
successful setter establishes completed rendering. Keep completed pixel work
in the separate control rather than labeling setter time as GPU execution time.

`completed-canvas-frames.html` is a separate integration control: 64 fixed-size
WebGL-to-Canvas frames for opaque, premultiplied and straight-alpha sources.
Each four-frame block includes a real `getImageData` completion barrier, and
final RGBA is checked for every source representation. Its number is explicitly
**not** the cost of one isolated `drawImage` or `readPixels` command.

Build the browser and `benchmarks/chromium/ChromiumBaseline.csproj` locally,
keeping build/profile/scratch output and `TEMP`/`TMP` on G: on this workstation.
Use a frozen executable for the before comparison. Serve the fixture directory
with `scripts/serve-alpha-fixtures.ps1`, then run:

```powershell
./scripts/compare-gpu-commands.ps1 `
    -BaselineBrowser <frozen-before-exe> -CandidateBrowser <final-release-exe> `
    -ReferenceAssembly <built-ChromiumBaseline.dll> `
    -FixtureBaseUrl http://127.0.0.1:<port>/ `
    -OutputDirectory G:/<task-artifacts>/command-comparison -Repetitions 3
```

The runner builds nothing and launches browsers sequentially. Breeze uses the
fail-closed hidden benchmark wrapper; the reference harness uses current
`--headless` and `CreateNoWindow`. Verify those launch settings before running
the harness. Each repetition rotates variant order and uses fresh profiles.
Temporary profiles stay below the explicit output directory. Raw reports,
screenshots, fixture/binary hashes, sample ranges and medians are retained.
No failed pixel check, incomplete title, or busy-machine outlier is dropped.
Cleanup errors remain visible even when measurement data was collected.

Avoid overlapping this agent's builds/full suites with timing runs. Other agents
may still compete for CPU, memory and GPU resources: report the range, not only
the median, and repeat an inconclusive result. Native diagnostic profiles also
report owner-thread CPU separately from elapsed time, but their instrumentation
overhead makes them attribution evidence, not ordinary speed measurements.
Neither a setter gain nor a frame-control gain proves full-game speed/parity.

Run `scripts/test-gpu-command-results.ps1` for the closed result grammar, unit
conversion and statistics tests. These benchmarks are local tools, not an
expanded CI standards/performance gate.
