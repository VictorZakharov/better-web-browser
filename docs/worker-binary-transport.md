# Dedicated-worker binary clone transport

Dedicated workers currently use the existing serialized structured-clone
envelope across their command/event queues. Window/frame messaging has a V8
ValueSerializer path; this change does not replace it or claim the worker
envelope already has all its contracts.

Binary fields retain their canonical padded base64 representation, so existing
stored envelopes do not need migration. Encoding and decoding now call bounded
native helpers instead of constructing one JavaScript character at a time.
The helpers use the existing locked `base64` 0.22.1 dependency (MIT OR Apache-2.0),
not copied codec code or a new dependency. V8 supplies actual view bounds and
copies only the selected bytes; author getters and iteration overrides cannot
fabricate backing storage. Native decode returns a real Uint8Array.

Each binary input and decoded allocation is limited to 16 MiB. String lengths
are checked in UTF-16 units and UTF-8 bytes before copying. Malformed encoded
input and over-limit payloads fail rather than creating unbounded allocations.
This per-field guard is not a claim that all worker-queue aggregate accounting
or serialization atomicity has been completed.

The bootstrap captures the byte-codec host function. Replacing public `atob`
or `btoa` no longer changes how the platform clones buffers, image pixels or
media snapshots. Public base64 API semantics are unchanged by this private
transport optimization.

Tests cover byte values, nonzero view offsets, malformed encodings, allocation
limits and three 4 MiB texture buffers cloned/transferred under the production
two-second watchdog. The dedicated-worker suite checks Canvas, codec, storage,
network and port interoperability. These tests do not prove the game runs;
application startup still needs its own unchanged-production-build capture.

The normative cloning contract is [HTML structured data](https://html.spec.whatwg.org/multipage/structured-data.html).
Typed arrays and DataView now serialize their backing buffer as an ordinary
graph edge, rather than repeating its base64 bytes for every view. Shared views
keep one independent cloned buffer, their original offsets and lengths, and
the same identity when the buffer also appears elsewhere in the graph. This
works whether a buffer or a view is encountered first and across transfers.
V8 supplies native view brands and slots; author `constructor`, `buffer`,
`byteOffset` and `length` getters are not consulted. Subclasses deserialize as
the corresponding intrinsic typed array. Intrinsic receiving constructors are
captured at bootstrap rather than selected through author-replaceable globals.
Old persisted view envelopes remain readable. Detached views and buffers fail
with `DataCloneError`; shared-memory worker transport remains unsupported.

Remaining platform-object, resizable-buffer metadata and transfer-commit work
must follow that contract, reusing V8 serialization where ownership/lifecycle
allow it. These changes do not turn the JSON transport into zero-copy transfer.
