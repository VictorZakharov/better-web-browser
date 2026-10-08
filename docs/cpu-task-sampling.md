# Explicit task CPU sampling

Ordinary task diagnostics report elapsed time, owner-thread CPU and GC callback
spans without sampling JavaScript stacks. They do not separate pure JavaScript
work from native bridge work on the same thread.

For deeper local diagnosis only, set `BREEZE_DIAGNOSTIC_CPU_SAMPLING=1` in the
task shell and enable the normal benchmark diagnostics. The default is off:
no CPU profiler is created in an ordinary run or merely by supplying selectors.
Remove the task-shell setting afterwards. All resulting captures/profiles belong
on G: and automated browser execution must remain hidden.
The renderer's contained environment admits this exact flag, not a general
diagnostic prefix or alternate V8-library settings. Normal containment remains
required for the capture.

The adapter uses the pinned V8 public CPU-profiler API, not private engine
layouts or a different V8 build. A recording lives entirely inside one entered
watchdog task. It is stopped and disposed before the entry guard exits, including
when Rust unwinds. Up to two completed tasks taking at least 500 ms emit source
summaries, separately from the first failed task per enabled epoch. Fast success
and exhausted-budget profiles are discarded without traversing their trees.
Draining diagnostics does not replenish either budget. Disable/re-enable starts
a fresh epoch, and repeated enable is idempotent.
Terminated microtask checkpoints are failures even though V8's checkpoint API
does not return a JavaScript error value and can clear its termination bit. The
sampler reads the watchdog's read-only cancellation state before leaving the
entered isolate. The watchdog still produces the public timeout.

The target sampling period is 2 ms with at most 2,048 retained samples. Windows
precise busy-wait sampling is disabled. Summary traversal is limited to 16,384
nodes and 128 candidate source coordinates, with at most 16 emitted rows.
Omitted-hit and traversal-truncation fields make partial summaries explicit.
Samples are statistical observations, not exact per-function milliseconds.

Each row contains only an engine-local numeric script ID, source line/column
and hit count. No function name, URL, source string, arguments, result bytes or
author object is copied into diagnostics. Numeric positions are useful for
mapping an already-owned local test bundle; IDs are not persistent across runs.
The summary also counts V8's public numeric source categories: script, builtin,
native callback, VM-internal, unresolved and an explicit future/other bin. These
counts partition retained hits without reading any function name or source
string. They help distinguish callback-heavy from VM-internal samples, but do
not separately identify compiler work or prove which operation caused a
watchdog failure. The summary is not a substitute for native-host timing.

The profiler itself changes runtime overhead and scheduling, and its setup and
teardown occur inside the unchanged two-second task boundary. Do not compare
its captures to unprofiled runs as a speed or memory benchmark. In particular,
a sampled failure does not prove that exactly the same work fails without
sampling. Use an unprofiled acceptance capture after implementing a change.

The C ABI transfers fixed numeric records and an opaque, uniquely-owned token.
The expanded profiler overload deliberately avoids `std::unique_ptr` parameters
across the prebuilt V8/MSVC standard-library ABI boundary. Both C++ and Rust
assert the record sizes; native lifecycle tests exercise repeated setup,
teardown and Rust unwind before isolate destruction.

API reference: [V8 CPU profiler](https://v8.github.io/api/head/classv8_1_1CpuProfiler.html).
