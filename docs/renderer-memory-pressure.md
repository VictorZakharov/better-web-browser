# Renderer memory pressure and engine boundaries

The renderer remains inside a broker-enforced Windows Job limit: 1 GiB by
default, or 2 GiB when the user selects the opt-in 3D budget in Options and
restarts the browser. The selection is persisted in the browser profile and
passed to the Job and child from the same typed launch policy; page code cannot
change it. Each renderer has its own limit, not a shared browser-wide allowance.
V8's individual heap limits cannot describe that process-wide budget: dedicated
workers, native image/Canvas resources, ANGLE, shader compilation and IPC storage
all consume the same process. This policy is an embedder signal to V8, not a
claim that one subsystem's ledger measures the entire process.

## Pressure coordination

The Windows process sampler reads private commit and working-set fields without
modifying handle ownership. An unavailable sample is not treated as zero memory
or evidence of recovery. A shared monotonic policy samples at most once every
250 ms and emits advice epochs consumed once by each V8 agent.

Moderate advice begins at 5/8 of the selected process limit; critical advice
begins at 13/16. Recovery uses hysteresis (below 3/4 to leave critical, below
1/2 to leave moderate). At the default 1 GiB these are 640/832 MiB and
768/512 MiB respectively. Stable high pressure does not repeatedly force
collections. A further 1/16 of the limit (64 MiB by default) growth may request
another pulse only after a five-second cooldown. Arithmetic
is checked or saturating; unavailable observations preserve the previous state.

Each agent delivers advice at a completed HTML task boundary after its Promise
checkpoint. The normal cancellation/entered-isolate guard surrounds V8's native
notification. No author callback, extra task pumping, kept-WeakRef clearing, or
increased script deadline is involved. Cancelled delivery does not acknowledge
an epoch. V8 decides which unreachable storage to reclaim; live ArrayBuffers
and author-visible objects must remain intact.

Opt-in diagnostics report notification counts, elapsed/max time and observed
process-private high-water separately from author execution and GC callback
spans. Worker pressure rows obey the existing bounded worker sample budget.
No URL, source, message payload, binary token, or author function name is logged.

## GPU allocation headroom

The native GPU ledger remains a conservative storage reservation, not exact
VRAM. Larger ledger ceilings require a fresh process-private headroom check
before positive storage growth. The renderer leaves 1/8 of its selected process
limit as emergency headroom (128 MiB by default). Reuse/retirement does not
require new headroom.
Unknown observations retain the old 64 MiB conservative allocation ceiling.

This is admission protection, not a peak-memory guarantee: a driver or compiler
may commit memory asynchronously after an accepted operation. Pressure hints,
headroom checks and immutable worker envelopes address different parts of the
same budget. They do not prove that an arbitrary page will fit or render fully.

## Owner-thread stack boundary

The browser executable already reserves an 8 MiB main-thread stack; owner threads
and test executables can have different stack sizes. V8's pinned default recursion
allowance cannot safely be assumed to fit
every owner after Rust/Win32 frames and native callbacks are accounted for.
The embedder queries the actual current-thread stack bounds and retains 256 KiB
of native headroom, without increasing V8's pinned maximum JS stack allowance.
Unknown, invalid, overflowed, or too-small regions fail context initialization
with an actionable error instead of constructing an unsafe address threshold.
The checked threshold is installed both in creation constraints and through
V8's public `SetStackLimit` after each guarded isolate entry. Entry failure
still retires the watchdog generation and balances isolate exit. Pending
termination is not cleared by installing this boundary.

The threshold belongs to the isolate's owner thread for its full lifetime.
Contexts/agents are non-Send, no fibers or stack switching are supported, and
the isolate is disposed before that thread exits. Small-stack tests require
deep recursion to produce a catchable `RangeError`, then verify normal evaluation
and Promise jobs still work. This boundary does not diagnose an unrelated
native compiler overflow or enlarge a thread's OS stack.

References: [V8 resource constraints and pressure notifications](https://chromium.googlesource.com/v8/v8/+/refs/heads/main/include/v8-isolate.h),
[Windows process memory counters](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex),
and [Windows thread stack bounds](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getcurrentthreadstacklimits).
