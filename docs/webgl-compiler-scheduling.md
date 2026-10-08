# Bounded native WebGL compiler scheduling

## Compatibility contract

Breeze reuses the already-pinned BSD-3-Clause ANGLE compiler and its public
worker-delegation interface. It does not substitute a custom GLSL/HLSL compiler,
copy upstream worker code, load another browser's libraries, or add a dependency.
The C++ adapter includes `mozangle` 0.7.1's public `PlatformMethods.h` and asks
ANGLE to validate the complete method-name signature before installing a callback.
Header discovery requires the exact package version; ambiguous custom registries
need an explicit `BREEZE_ANGLE_SOURCE_DIR`, just as the V8 adapter does.

The native parallel-compilation capability is enabled privately at context
construction. This is browser scheduling policy, not author API admission.
`COMPLETION_STATUS_KHR` still requires the page to request
`KHR_parallel_shader_compile`; the public API never exposes native compiler
thread-count controls. Completion is the real native state and does not imply
compile/link success. Blocking success, reflection and use queries retain their
native dependency waits. Small shaders may complete before their first poll.
The [KHR contract](https://registry.khronos.org/webgl/extensions/KHR_parallel_shader_compile/)
expressly allows parallel compilation even without author extension enablement.

## Ownership and bounds

- One renderer-wide pool has four workers, independent of the number of native
  contexts or the host's CPU count. Each reserves an eight-MiB stack, matching the
  existing native owner; Windows commits stack pages on demand.
- The mailbox holds at most 128 queued native closures. A full queue applies
  backpressure rather than admitting an unbounded work list. A disconnected
  mailbox runs the returned closure once so its native completion event cannot
  leak or become permanently pending.
- Only trusted ANGLE callbacks and opaque ANGLE-owned closure pointers cross
  this interface. ANGLE retains task resources and handles link/compile waits.
  Author strings, WebGL object IDs and native context handles cannot choose a
  callback or worker entry point. Tasks execute outside the mailbox lock.
- The pinned provider stores its platform methods process-wide and can reset
  them during display initialization. The single GPU owner waits for all queued
  and running native tasks before initializing another display/installing its
  delegate, avoiding concurrent mutation of that provider-owned structure.
- Worker threads live until renderer process exit; shutdown does not join them
  under Windows' TLS loader lock. Context/object teardown still uses ANGLE's
  native pending-work lifetime rules. Renderer Job termination remains the
  hard cancellation backstop for native driver/compiler calls; JavaScript
  interruption cannot preempt an arbitrary native call.

The existing source, object and storage limits remain in force. This change
does not raise the production GPU admission budget or the ten-second author-task
deadline. Compilation validation in Breeze's WebGL validator remains synchronous
and bounded; ordinary submission/front-end validation can still consume time.
Backpressure and queries that actually need compiled code can still block.

## Regression coverage and measurements

### Owner-side link ordering

The pinned ANGLE front end resolves attached shader translations during
`glLinkProgram`, before posting the later native executable-link jobs. Calling
it immediately after every shader pair therefore serializes translation work.
Breeze keeps up to 128 pending link submissions per context, each with at most
the two WebGL shader stages. Submission stays on the sole GL owner: Rust compiler
workers never acquire a context or call a GL entry point.

Ready translations allow the owner to submit actual native links at idle turns,
task boundaries, completion polls, and while later shader pairs are submitted.
Each such progress operation advances
at most one invocation per context. `COMPLETION_STATUS_KHR` is false while its
real invocation remains queued, and otherwise comes from ANGLE. No compiler
success, elapsed-time completion, or artificial delay is invented.

Non-blocking progress first checks that the delegate mailbox has room for all
three executable jobs posted by the pinned D3D graphics linker. This check is
conservative because its pending count includes active workers. The sole GPU
producer cannot consume those slots between the check and ready-link submission;
compiler workers only release slots. Saturated background work therefore leaves
the invocation queued instead of making a completion poll wait on backpressure.

Shader source/recompilation, program attachment and attribute/capture changes,
relinking, use, reflection, logs, and deletion resolve the relevant preceding
submission prefix first. Shared-shader mutations resolve every earlier program
that refers to that shader. Uniform locations expire when the browser receives
the relink, not later when ANGLE receives it. Current-program relinks remain
immediate. Explicit flush/finish/error and native extension/capture barriers
cannot overtake pending links. Queue saturation applies bounded backpressure.
Unobserved submissions are discarded with their context; normal object teardown
still owns every native name and pending ANGLE task.

### Completed native event retirement

The pinned `Shader::isCompleted` and program completion queries inspect event
readiness but do not themselves release compilation/linking payloads. Native
`Shader::resolveCompile` returns the compiler instance and clears its compiling
state; `Program::resolveLinkImpl` releases the completed linking state while
installing the actual executable. Retaining many completed but unresolved events
can therefore retain temporary storage even after worker execution finishes.

The GL owner tracks at most 64 unresolved shader events and eight program events
per context. Idle turns, task boundaries, and later submissions inspect a bounded
number of events. Only a native-ready event is resolved by a non-blocking path.
Resolution preserves source, logs, reflection, uniform-location generations and
the native success/failure result; it never deletes an author's executable.
Submission saturation applies ordinary bounded backpressure rather than letting
temporary event ownership grow with the number of materials. Recompilation and
relinking replace the recorded invocation. Deleted browser objects are checked
before any native query so recycled GL names cannot alias a retired event.

These limits bound event counts, not their exact byte sizes. ANGLE/compiler/driver
working sets and caches still need measured pressure testing, and the renderer's
unchanged 1 GiB Windows Job remains the hard memory/cancellation backstop. They
are not evidence that a larger production GPU budget is safe.

Deterministic pool tests keep closures blocked behind a test gate and verify that
other submissions return while they remain pending, worker concurrency is fixed,
queue overflow blocks without dropping tasks, and mailbox closure drains owned
work. A native regression test requires actual ANGLE delegate submissions in
addition to successful/failed completion queries; the former synchronous build
policy cannot satisfy that assertion.

`tests/webgl/material-warmup.html` compiles and links 24 owned, distinct materials,
polls actual completion without querying link success early, checks every link,
draws every program and verifies native pixel readback. It reports submission
and completion separately, without requiring an artificial pending interval.
The unchanged sibling game is a separate acceptance check, not a replacement
for these standards/lifetime tests. Performance and game results must be recorded
from final release builds before claiming startup gains.

### Intermediate release comparisons, October 8

Three fresh-profile runs per browser used the same owned 24-material fixture,
device scale 1, 1262×539 viewport and ten seconds of settling. Run order alternated;
all launches were hidden, CPU sampling was off, and no builds ran concurrently.
The before build includes this batch's earlier Canvas/cancellation/accounting
changes but retains synchronous ANGLE workers; the after build adds delegation.
Both keep the production 64/128-MiB GPU limits and ten-second task deadline.
These are intermediate-build observations, not final-PR-head results.

| Median measurement | Breeze before | Breeze after | Chrome 154 |
| --- | ---: | ---: | ---: |
| Submit 24 shader/program pairs | 539.1 ms | 144.2 ms | 1.0 ms |
| Actual completion of all programs | 539.9 ms | 180.2 ms | 113.4 ms |
| All 24 links and native pixel assertions pass | 3/3 | 3/3 | 3/3 |
| Genuine pending completion observed | 0/3 | 3/3 | 3/3 |

Completion improves about threefold in this fixture. Breeze's submission still
costs much more than Chrome's; synchronous bounded WebGL validation and bridge
round trips remain. This is not a claim of Chrome-equivalent game startup,
universal shader speed, pixel-perfect rendering or additional HTML5test points.

Owner-side queuing alone reduced submission time but regressed total completion:
in a separate matched comparison its median submission/completion was
46.9/197.7 ms, versus 128.7/160.1 ms with the delegate alone. That intermediate
result was not accepted as an overall startup improvement. Pipelining ready
links during later submissions and retiring completed native payloads then
produced this separate three-run matched comparison:

| Median measurement | Delegate only | Queue, pipeline and retirement | Chrome 154 |
| --- | ---: | ---: | ---: |
| Submit 24 shader/program pairs | 135.9 ms | 62.3 ms | 1.0 ms |
| Actual completion of all programs | 166.2 ms | 161.7 ms | 113.8 ms |
| All links and native pixel assertions pass | 3/3 | 3/3 | 3/3 |
| Genuine pending completion observed | 3/3 | 3/3 | 3/3 |

The roughly 54% submission reduction is distinct from the small completion-time
difference, whose individual observations overlap. It does not establish a
significant total-speed improvement over delegation alone. These fixtures use
the production memory limits, but do not establish unchanged-game acceptance.

### Larger-budget experiment is not a production policy

The separately authorized 256/512-MiB GPU-budget experiment keeps the renderer's
1-GiB Job limit. With queuing alone, both fresh 30-second unchanged-game captures
retain the loading overlay. The attributed run ends at 988.1 MiB private memory;
the ordinary run reaches 1,022.5 MiB and its stderr reports a failed two-MiB
allocation. Those are final private-memory samples, not peak-private or VRAM
measurements. A report with no recorded JavaScript exception is not evidence of
successful startup when the screenshot and allocation failure say otherwise.
The production budgets remain 64/128 MiB. The subsequent retirement replay still
retains loading in both captures, with final private samples of 966.9/992.3 MiB
and peak working sets of 883.1/912.4 MiB. Neither run records an allocation-failure
message, but the private-memory samples remain close to the unchanged Job cap.
Two observations without that message do not prove reliable memory headroom or
unchanged-game completion.

### Optional native driver debugging layers

Display creation explicitly supplies `EGL_PLATFORM_ANGLE_DEBUG_LAYERS_ENABLED`
as false for hardware and WARP. The pinned provider otherwise prefers available
debug layers when its assertions are compiled in; a hidden game renderer on the
development machine loaded the D3D SDK-layer DLL under that default. This is a
production display-policy choice, not a change to native shader optimization.
It does not disable ANGLE assertions, GLES/WebGL validation, robust resource
initialization, client-array restrictions, or browser-side range/type checks.
The [ANGLE platform extension](https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_platform_angle.txt)
distinguishes these optional backend validation layers from WebGL API admission.
DLL loading alone is not a measurement of their memory or time overhead; gains
need matched release replays rather than an assumption about installed SDKs.

### Explicit validator reuse

The browser still runs ANGLE's explicit WebGL validator before every uncached
native compilation. Each GPU-owner context keeps at most one vertex and one
fragment validator handle. A slot is rebuilt when its exact WebGL version or
extension environment changes; handles, resources, diagnostics and symbols are
never shared between contexts, stages or threads. This reuses the existing
`mozangle` dependency rather than introducing another shader parser.

Repeated compile calls are supported by the pinned
[ANGLE compiler API](https://github.com/google/angle/blob/main/include/GLSLANG/ShaderLang.h).
Its `TCompiler::compileTreeImpl` clears prior results before parsing, and
`TScopedPoolAllocator` releases the compilation tree after each call. The browser
copies real generated output or diagnostics before returning; no previous
author result is substituted. Failed input retires its handle. Successful input
over 128 KiB, oversized translated output, or 16 uses also retires the slot, so
native high-water allocator capacity is not retained for the context's lifetime.
These are reuse limits, not new limits on otherwise admitted legal shaders.

Native tests compare reused and freshly constructed validator output across
distinct author symbols, both stages, WebGL versions and extension transitions.
They also cover failure recovery, large-input retirement and new/restored owner
isolation. The existing successful-output cache is independent: its hits still
compile a real native shader, and neither mechanism caches native compile/link
status. This change makes no assumption that validator reuse solves the game's
remaining native memory/startup pressure; that requires separate release runs.

Retained validator handles are explicitly destroyed **before** native context
teardown. The pinned native compiler finalizes shared translator TLS when its
last native compiler is destroyed; a later validator destructor would access
that finalized allocator. A native regression repeatedly drops the sole context
with both completed and outstanding compilation, then recreates WebGL 1 and 2
contexts. Direct translator-only tests are insufficient to exercise this native
compiler lifetime boundary.

### Final-source release comparison

Three fresh, alternating runs compare merged #229 with the complete October 8
source release and Chrome 154. CPU sampling was disabled, no builds/tests ran
concurrently, and all browsers used the same 1262×539 viewport at scale 1.
The fixture is unchanged across all nine captures.

| Median, 24 distinct materials | Merged #229 | October 8 release | Chrome 154 |
| --- | ---: | ---: | ---: |
| Submit shader/program pairs | 532.3 ms | 52.1 ms | 0.9 ms |
| Actual native compiler completion | 533.3 ms | 163.0 ms | 104.8 ms |
| All links and subsequent pixel checks pass | 3/3 | 3/3 | 3/3 |

Compiler completion uses actual native completion-status polls. Every completed
program is subsequently drawn and read back, but those pixel checks are outside
the compiler-completion timer. Individual completion observations are
554.9/533.3/509.6 ms before, 156.2/178.0/163.0 ms after, and
100.6/104.8/106.2 ms in Chrome. The combined batch improves this fixture roughly
3.3-fold, not 10-fold: submission and completion are different milestones.
It remains slower than Chrome and does not establish unchanged-game readiness.
