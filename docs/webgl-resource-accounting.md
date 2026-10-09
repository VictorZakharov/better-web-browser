# WebGL storage admission and retirement

The standard renderer policy admits at most 256 MiB per context and 512 MiB
per native owner, inside the default 1 GiB Windows Job limit. The opt-in 3D
policy admits at most 1 GiB per context and owner, inside a 2 GiB Job limit.
Both policies reserve process-private emergency headroom before storage growth;
see [renderer pressure and headroom](renderer-memory-pressure.md). User selection
requires a restart and does not grant any new JavaScript capability.

GPU-object and uniform-location counts are separate limits. Storage accounting
is a conservative reservation, not a claim to measure the driver's exact VRAM.
Two copies cover native storage and browser mirrors used to validate draws.

Each live buffer, shader, texture image, and renderbuffer keeps a high-water
reservation. Repeating or shrinking a definition does not charge the entire
allocation again; growth charges only the difference. This is especially
important for reusable render targets and multisample renderbuffers.

Admission validates an immutable reservation before allocating native storage.
The browser commits its capacity and counter only after the synchronous driver
operation succeeds. Invalid arguments, exhausted budgets, and abandoned
reservations do not alter the browser's mirror or grant additional storage.
Texture definitions keep their existing per-mip transactional reservations.

Deletion is not synonymous with retirement. A deleted buffer may remain in an
inactive VAO, a texture or renderbuffer in an inactive FBO, and a shader in an
attached or current program. The existing typed reference registry determines
when the last reference is released. Only then does the record retire and return
its reservation. The command boundary drains all retirements, including nested
program/shader releases and deletions routed through WebGL 2 object dispatch.
Repeated deletion cannot reclaim twice. An accounting underflow fails closed
rather than admitting untracked storage.

This fixes the previous lifetime-total counter, which charged resources after
their actual retirement and could eventually reject an otherwise bounded
allocation/release workload. The later policy increase is separate: renderer
Job and ledger experiments establish whether a page actually fits and renders,
not whether its JavaScript merely survives an allocation failure. There is no
site-specific exception, URL-dependent policy or successful fake allocation.

Native tests cover buffer churn under a small budget, delayed VAO/FBO/program
retirement, repeated and shrinking renderbuffer definitions, multisample
redefinitions, failed admission, and unchanged native sizes after rejection.

## Opt-in attribution

Existing runtime memory diagnostics also collect native-owner ledger snapshots.
They report charged storage, the effective context limit, private drawing-buffer
storage, live/pending-delete object counts, and high-water capacities by resource
type. Capacities are unmultiplied; charged storage includes the conservative
two-copy multiplier and private attachments. They are not actual driver VRAM.

Each context retains only a saturating rejection count and the first/latest
failed storage admission, including the charged/additional/limit byte values.
This fixed-size history distinguishes browser admission from native driver
errors without retaining uploads, shaders, URLs, or an unbounded event log.
Arithmetic-overflow admissions also fail closed. Other validation failures and
native driver errors do not increment the storage-admission counter.

The snapshot is an embedder-only request to the existing owner thread. It
performs no GL call, readback, getError, or object retirement. It is not a new
JavaScript extension, getParameter value, or author-visible fingerprint surface.
The existing diagnostic sample budget controls when it is requested; ordinary
unprofiled tasks do not add a diagnostic owner round trip. Native tests preserve
the sticky error set across repeated snapshots and exclude peer/retired contexts.

Reference: [WebGL resource deletion](https://registry.khronos.org/webgl/specs/latest/1.0/#5.14.1)
and the corresponding [WebGL 2 object operations](https://registry.khronos.org/webgl/specs/latest/2.0/).

## Controlled larger-budget experiment

The following October 8 observations describe the previous release and its
then-unchanged production policy. They are historical evidence, not the current
limits or an assertion that the game now fits the standard renderer budget.

A local October 8 experiment compared two frozen intermediate builds differing
only in the context/owner limits: 64/128 MiB and 256/512 MiB. The renderer Job
limit remained 1 GiB. Both ran the unchanged sibling-game snapshot with fresh
profiles, scale 1, hidden execution, and 30 seconds of settling. Each build had
one private-ledger attribution run and one ordinary run; these are observations,
not medians or final-PR-head measurements.

| Observation | 64/128 MiB | 256/512 MiB |
| --- | ---: | ---: |
| Storage rejections, attribution run | 5,840 | 0 |
| Shader console errors, either run | 125 | 0 |
| Uncaught errors, either run | 0 | 1 |
| Final renderer private memory, ordinary run | 685.3 MiB | 692.3 MiB |
| Peak renderer working set, ordinary run | 775.7 MiB | 740.2 MiB |
| Complete rendered game lobby | No | No |

The larger budget allows real compiler work instead of allocation failures, but
the game then aborts a promise job at the ten-second task deadline and remains
on its loading screen. The profiled failed task lasted 10,152 ms while consuming
1,328 ms of document-owner CPU. This is not total renderer CPU: the native GPU
owner is a separate thread. These measurements motivate attribution of native
compiler/command waits, not an assertion that JavaScript alone is slow or that
an even larger watchdog deadline is safe.

At that abort the ledger charged 134.9 MiB with only 162 live objects, so it does
not measure the complete game's peak GPU requirements. Neither run reported a
renderer exit, but final private bytes and peak working set are not peak private
bytes or a guarantee of future headroom. The production defaults remain
64/128 MiB pending a complete, bounded rendering/cancellation assessment.

The complete final-source release is also replayed under the separately
authorized 256/512-MiB experiment. Its first ordinary capture crashes after
30.5 seconds with a failed two-MiB allocation under the unchanged one-GiB Job;
the repeat set is stopped at that failure. Zero renderer-memory fields after
exit do not measure its peak or prove headroom. The controlled experiment is
not shipped: normal source is restored byte-for-byte to 64/128 MiB.
