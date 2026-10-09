# Bounded WebGL numeric commands

Common void WebGL setters use an owned numeric command instead of repeatedly
encoding and parsing JSON. This is private transport, not a new extension,
JavaScript method or relaxed validation policy. WebGL 1, WebGL 2, Window and
dedicated Worker retain the same public argument conversion and GPU behavior.

## Conversion and ownership

The existing Web IDL layer runs conversions, sequence iteration and typed-view
snapshotting in their specified order. Only afterward does the bridge consider
the numeric path. Its closed operation set excludes queries, string payloads,
uploads and readback. Commands with more than 64 numeric values use the ordinary
bounded transport. Integer values must be safe JavaScript integers; unrestricted
float values retain negative zero, NaN and infinities.

The native boundary checks the operation, exact argument count, context ID,
primitive array/Float32Array brands and both lengths before reading any element. It never
coerces an object or accepts a proxy array. Both lengths are frozen before
property reads, so a malformed getter cannot grow a later allocation. A changed
length rejects the command. Native vectors own the copied primitive values; no
author backing buffer, V8 handle or borrowed JavaScript storage crosses threads.

Small floating-point uniform ranges use the existing public V8 `CopyContents`
API to copy their exact byte-offset range into a bounded Rust buffer; no second
JS component-boxing loop is needed. WebGL 2 may defer the independent snapshot
until that native copy only for a fixed, non-shared Float32Array of at most 62
elements, in an installed uniform slot whose remaining arguments are numbers or
undefined. The remaining intrinsic conversions cannot execute author code.
Sequences, shared or larger sources, and object/string offset or length arguments
retain the early independent IDL snapshot. In particular, a later getter that
mutates or detaches the source cannot change the values already converted.
Direct malformed private calls reject shared, resizable, detached and
proxy views, and recheck detachment after integer getters. Ordinary numeric arrays
and larger uploads retain their existing paths. Private method dispatch and range
selection use captured intrinsics without consulting typed-array species hooks.
Small ranges get a private fixed view into the admitted buffer, without
consulting author buffer/length/offset getters or iterators. Fixed interface
conversion slots are prepared once when the method is installed, but conversion
order and dynamic overload selection remain unchanged. Private argument vectors
have null prototypes: omitted optional arguments are undefined, not inherited
Array.prototype numeric properties. Scalar conversions use captured intrinsics,
so replacing Number, BigInt or Math helpers cannot introduce a new coercion hook.
Empty non-null uniform
uploads retain WebGL's minimum assigned-type length check; a null location
continues to ignore the converted data.

Property reads happen before a HostState borrow. Native dispatch rechecks realm
liveness and context ownership afterward, including after a reentrant getter
destroys the context. The real bootstrap bridge is lexically captured and removed
from both author globals. Module completion uses native V8 settlement and
captured private handlers, never a temporarily global pending Promise or native
callback. It does not consult author constructor/species hooks; see the
[settlement contract](private-module-settlement.md).

## Ordered execution and errors

The trusted bootstrap first groups small converted setters into a private,
fixed-capacity Float64Array. A checked-in operation table is shared with the
native decoder, so operation numbers cannot silently drift between the two.
Each record carries its context ID and integer/float counts; at most 128 records
and 64 arguments per record occupy 8,704 doubles (68 KiB). This reduces crossings
of the JavaScript/native boundary, not the number of submitted GL operations.
Uniform values are copied into the packet at their public call, before an author
can mutate or transfer the source. Queries, uploads, fallback commands, snapshots,
creation and context retirement submit the packet before proceeding.

A captured Promise reaction submits a remaining tail at the microtask checkpoint.
Its private promise has an own constructor/species record: replacing author
Promise hooks cannot redirect scheduling. Submission is not GPU publication;
the existing task boundary still waits for the native owner after nested jobs.
The native bridge copies the fixed view's used prefix without author property
reads, rejects shared/resizable/detached storage and decodes the entire packet
before admitting any operation. A malformed tail therefore admits nothing.
Each admitted record still passes through normal realm ownership, command
validation, resource accounting and context-loss reporting. The return array
initializes own elements without invoking author Array.prototype setters.

Numeric and JSON entries share a bounded 128-command pending queue. A numeric
entry contains at most 512 bytes of numeric payload, plus fixed record overhead;
JSON entries retain their separate 1,024-byte batching admission bound. Larger
ordinary commands keep their existing overall command and upload limits.

One submitted batch may overlap JavaScript preparing the next bounded list. A
realm retains at most one outstanding reply plus one 128-command pending list;
it waits for the previous reply before submitting another. The native owner still
receives every operation through its existing single bounded FIFO channel.
Queries, uploads, readback, profiling and task boundaries drain both lists in
order. Retiring a context cancels its unsent work and releases its native lease;
other contexts keep their relative command order. Clearing a realm drops its
outstanding reply without waiting for GPU work. Owner-side orphan cleanup and
lease retirement prevent cancelled operations from leaving native objects alive.

Each submission retains its original three-second deadline. Polling cannot reset
that deadline, and a timeout, disconnect or unexpected reply retires only that
batch's submitting-realm IDs. A ready reply is consumed without an extra wait.
Ordinary queued setters may return before native execution; observations and
trusted task publication cannot see commands out of order or fabricated results.
Both transports call the same native command validator,
resource admission, context activation and dispatch. Invalid enums, dimensions,
uniform shapes and object lifetimes remain ordinary sticky GL errors. An absent
or lost native context still becomes context loss; an invalid public GL operation
is not converted into context loss just because its void reply is omitted.

Within one synchronous native-owner batch, consecutive converted numeric
commands for the same context share one activation check. This is not a cached
EGL identity across requests: the proof starts empty on every batch, and a
context change, JSON fallback or lost/poisoned context ends it. Only the closed
numeric setter type can use the proof; uploads, observations, creation,
retirement and compiler/task checkpoints retain their ordinary activation
paths. Every setter still executes native validation, accounting and dispatch.
The owner thread cannot run a peer between those consecutive records.

The bridge therefore avoids serialization work without skipping native draws,
driver calls, task-boundary publication or resource accounting. A synthetic setter
gain does not establish game startup acceleration. Use the
[small command comparisons](gpu-command-benchmarks.md) for optimization and
keep unchanged-game loading/rendering checks separate. Shared-machine scheduling
can change elapsed time even when the command implementation is unchanged.

## Regression coverage

Tests exercise native operation admission, bounded arrays, unsigned uniform bits,
special floats, poisoned author JSON hooks, mixed transports, cross-context order,
binary upload/deletion barriers and real shader draw/readPixels results. Public
tests cover sequence and typed-view conversion, reentrant getters, context
retirement, and large matrix lists that must use the ordinary transport. Worker
tests ensure that replacing an author-global bridge or Promise.prototype.then
cannot forge module completion or break queued-message readiness.

Deterministic submission tests cover deadline retention, stalled/disconnected
replies, cancellation and partial-context loss. Native pipeline tests assert real
errors, colors, buffer uploads, binary reads, task barriers and cross-context
retirement after full submitted batches, rather than only inspecting queue lengths.

References: [WebGL 1](https://registry.khronos.org/webgl/specs/latest/1.0/),
[WebGL 2](https://registry.khronos.org/webgl/specs/latest/2.0/),
and [Web IDL conversion](https://webidl.spec.whatwg.org/#js-type-mapping).
