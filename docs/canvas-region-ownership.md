# Canvas region ownership and startup diagnostics

This slice reduces copies at the existing native Canvas boundary. It does not
change path tessellation, antialiasing, paint ordering, the script watchdog or
the browser's resource budgets.

## Byte ownership

The V8 bridge still copies author-provided bytes into an independent native
allocation before admitting a host call. Solid-path painting can consume that
allocation, composite into it and return its ownership to V8. It never takes
ownership of, writes through or retains the author's ArrayBuffer. Other host
calls keep their established borrowed contract. Window and Worker host liveness
checks remain in force before painting.

The prepared solid source-over operation hoists calculations shared by pixels
with the same coverage. Its operation order matches the preceding scalar path.
Exhaustive scalar comparisons cover all coverage/source/destination channel
combinations used by the prepared arithmetic; opaque and transparent shortcuts
are tested separately. The optimization does not substitute a different
rounding or alpha convention.

## Bounded scratch storage

Native coverage retains at most 8 MiB of scratch storage on its owner thread.
The preceding dirty region is cleared before reuse. Conservative control-point
bounds include the antialiasing fringe; invalid or uncertain bounds fall back to
clearing the full mask. Dimension/stride changes and nested painting cannot
reuse an incompatible active mask.

The private JavaScript bootstrap retains at most one 2 MiB pixel region per
realm. Taking it removes it from the pool before invoking the native operation,
so nested work cannot alias the active region. Every destination byte is
overwritten from the current Canvas bitmap before painting. Only a valid,
correct-length native result is committed to the bitmap; failure leaves the
bitmap unchanged. Scratch release runs in `finally`. Full-width row regions use
one contiguous copy; narrower regions preserve the destination stride.

These are allocation pools, not cached paint results. Changing the bitmap,
clip, paint, dimensions or source data cannot reuse an old visual result.
Private captured typed-array operations remain independent of author prototype
changes. Unit and Window/Worker tests exercise reentrancy, reset, failed updates,
retention limits and author aliasing.

## Clipped source shaders

Linear, radial and conic gradients, and repeating bitmap patterns, now admit
the existing packed bitmap clip in the native shader batch. Clip membership is
applied to final compositing, after source sampling and path coverage. The
shader's bitmap origin must exactly match the clip lookup's region origin;
malformed dimensions, packed length or coordinates decline atomically to the
existing fallback. A missing clip preserves prior off-bitmap sampling behavior.

Window and Worker tests check every destination byte against independent clip
membership, including nested intersections, translated paths, opacity, fill and
stroke, save/restore and resize. Solid arithmetic's independent scalar oracle
uses 257 constant gradient stops, beyond the native 256-stop admission budget.
Pattern arithmetic additionally compares against 32-pixel scalar rows, below
the 256-pixel batching threshold. A clip no longer implicitly forces fallback.

This expands admission to the existing tiny-skia source shader; it does not
make its floating-point interpolation and 8-bit source quantization identical
to the JavaScript fallback in all translucent cases. Exact solid arithmetic
assertions remain unchanged. `tests/canvas/clipped-shader-throughput.html`
separately checks unchanged hole/outside pixels and opaque interior coverage,
stable per-engine checksums, warmup and a readback fence for each shader kind.

Three fresh hidden profiles per browser, rotating Breeze order, give the
following medians of nine warmed 128-square batches. Each batch paints eight
opaque rectangles through a rectangular clip with an even-odd hole and reads
the resulting bitmap. The before column isolates the shader extension: it is
the preceding normal batch build, already containing region ownership changes.
No compilation runs concurrently with these measurements.

| Clipped shader batch | Before extension | After extension | Chrome 154 |
| --- | ---: | ---: | ---: |
| Linear gradient | 9.7 ms | 1.6 ms | 0.4 ms |
| Radial gradient | 12.0 ms | 2.0 ms | 0.3 ms |
| Repeating pattern | 16.4 ms | 1.9 ms | 1.8 ms |

Breeze checksums match before/after in every run: linear `4231392064`, radial
`3151033856`, pattern `2879782912`. Chrome's gradient checksums differ but remain
stable (`2382961392` and `4134737984`); its pattern checksum matches Breeze.
Every run separately validates the clip's unchanged transparent pixels and
fully opaque interior. This workload does not establish all-gradient pixel
parity, GPU rendering speed or successful game startup.

## Solid-path measurement

`tests/canvas/region-throughput.html` draws sixteen curved strokes per batch on
a 512-square Canvas. Each timed batch includes a readback fence; the fixture
checks the whole resulting bitmap. Four batches run per fresh browser profile,
with the first treated as warmup. The context requests `willReadFrequently` so
Chrome does not switch from GPU to CPU storage between checksummed batches.
This is deliberately a CPU-oriented, readback-heavy workload, not a general GPU
Canvas or browser ranking.

| Nine warmed batches across three fresh runs | Before (#228) | After | Chrome |
| --- | ---: | ---: | ---: |
| Median batch time | 18.8 ms | 10.5 ms | 1.2 ms |
| Completed batch range | 18.0–23.9 ms | 10.3–12.9 ms | 1.0–1.4 ms |
| Stable whole-bitmap checksum | 1672046960 | 1672046960 | 405226884 |

The Breeze median improved about 44%, but Chrome remains substantially faster
on this fixture. Breeze's before/after pixels are identical for every batch.
Chrome's separate checksum is stable but different: the comparison does not
claim cross-engine antialiasing parity. Initial reference captures without the
storage hint changed checksum between batches; those were rejected, not included
as successful comparisons or addressed by weakening the assertions.

## Opt-in worker attribution

Renderer diagnostics can collect elapsed message-task time, Windows owner-thread
CPU time, V8 GC callback spans/counts and numeric heap statistics. They do not
record author arguments, results, source or function names, and do not appear
as author console messages. Successful task logging is bounded to eight samples
per worker, with a separate single sample reserved for its first failed task.
Repeated failure cannot grow the profiling log indefinitely. Diagnostics disabled
means no GC hook or profiling sample storage. Queued messages awaiting module evaluation
are not currently measured by the immediate-dispatch profiler.

V8 callback data lives in stable boxed storage on the isolate owner thread.
Both callbacks are removed while the isolate is entered, before the box or
isolate is destroyed. Callbacks allocate no memory and execute no JavaScript.
Callback spans do not account for concurrent background GC. Windows CPU totals
have OS-dependent resolution, so short samples may round to zero or exceed an
elapsed interval slightly; they are diagnostic, not deadline enforcement.

These diagnostics help distinguish computation from descheduling and collection.
They do not extend the unchanged two-second execution limit or establish that
the unchanged game's loading pipeline is now accepted. The GPU breakdown uses
fixed operation categories; command/shader source bodies are not recorded by
that breakdown.

Document diagnostics additionally sample existing watchdog engine entries.
These are engine calls/checkpoints, not necessarily complete HTML tasks: the
elapsed and owner-thread CPU totals include native bridge work. Eight completed
entries of at least 16 ms can be recorded per shared document Agent, plus one
reserved failed entry. Short initialization/property reads do not exhaust this
budget. Draining the log or repeatedly enabling related realms does not
replenish it; explicitly disabling and re-enabling starts a new diagnostic
epoch. Disabled profiling installs no GC callback and samples no OS CPU time.
No author source, arguments, results, function names or error text are retained
by this profiler. CPU/GC attribution never changes deadline enforcement.
Owner-thread CPU does not include the separate ANGLE owner thread or other
workers. Native bridge waits contribute to elapsed time; correlate them with
the fixed host-call breakdown rather than assuming every elapsed/CPU gap is
scheduler delay.
