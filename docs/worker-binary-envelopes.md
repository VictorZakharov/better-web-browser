# Dedicated-worker binary transport

Window ↔ dedicated-worker `postMessage` uses the existing structured-clone graph
implementation with a native binary envelope. Binary content is not expanded
into base64 or duplicated into each queue's JSON string. Persistent storage,
History, local `structuredClone`, BroadcastChannel and deferred MessagePort
delivery retain their existing wire format. This is not zero-copy transfer:
the receiving realm still gets independent mutable V8 backing storage.

## Ownership and compatibility

The trusted bootstrap serializer walks graph objects and runs transfer steps
in their existing order. One backing buffer is represented once even if many
typed views reference it. Cycles, repeated references, view offsets and lengths,
platform-object records, transferred ports, and sender detachment keep the same
graph contract. Binary transport does not invent a new author-visible type.

An envelope owns immutable byte vectors and graph metadata through one `Arc`.
Native queue copies clone that owner rather than copying the data. No V8 value,
persistent handle, context, or thread-local reference crosses an OS thread.
The envelope can outlive the source realm; its last native owner releases its
process-wide storage reservation. Wrapping an owned vector does not perform a
second native byte copy into an `Arc` slice.

The receiver temporarily installs a reader around its captured dispatch hook.
Each private binary token identifies an envelope nonce and index. Only a token
belonging to the current native envelope can be consumed, once per delivery.
Tokens are never usable through a guessed metadata-only string. Each delivery
creates fresh receiver backing storage; two deliveries cannot share mutable
ArrayBuffer bytes. The reader ends before the later Promise checkpoint.

Temporary serializer/decoder exports are removed before author scripts run.
The native serializer capability and dispatch hooks are captured privately,
not reread from page-replaceable globals. Persistent and reentrant local clones
never produce ephemeral tokens. MessagePort decoding can be deferred into a
later task, so it deliberately remains on the persistent-compatible path.

## Bounded allocation and failures

The existing 16 MiB per-binary limit remains. A whole envelope, including graph
metadata, binary entries and a fixed owner charge, is limited to 32 MiB.
Across the renderer, pending native captures/envelopes retain at most 128 MiB
and 1,024 messages. Writer/reader nesting and binary-entry counts have separate
finite bounds. These are implementation resource limits, not promises about
available JavaScript heap or driver memory.

The sender reserves native storage before copying binary bytes. Large byte
allocations are fallible and surface `DataCloneError`; partial captures unwind
on exceptions, watchdog interruption, and native unwinding. Internal ownership
or accounting violations fail closed. Quota checks do not grant extra process
memory or silently discard a successful message.

Mailbox accounting charges the entire envelope, not only its small metadata.
The process-wide lease also covers messages awaiting module initialization and
worker responses waiting for the document event loop. Termination or navigation
can retire a stale message without invoking the former owner's author callback.

Graph getters may post reentrant messages, terminate a Worker, or close their
own worker. No host-state borrow spans serialization. Ownership is checked again
afterward before enqueueing. Transfer steps remain observable in list order;
there is no rollback promise if a later transfer/resource step fails.

## Verification

Tests exercise actual Window/worker delivery, native reader ownership and quotas,
cycles/shared views, transfer getter ordering, reentrant local and worker clones,
captured callbacks, malformed envelopes, delayed module dispatch, termination,
and independent receiver mutations. A three-map 12 MiB payload keeps its exact
bytes with less than 1 KiB of graph metadata; that assertion is a transport-size
contract, not a game-startup timing or memory benchmark.

References: [HTML structured data](https://html.spec.whatwg.org/multipage/structured-data.html#structuredserializewithtransfer)
and [Worker posting](https://html.spec.whatwg.org/multipage/workers.html#dom-worker-postmessage).
