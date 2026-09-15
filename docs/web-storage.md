# Web Storage values, persistence, and cross-tab events

Breeze implements the value/mutation contract for `localStorage` and `sessionStorage`, plus
same-origin cross-tab synchronization within one browser application. This is not a claim of
full Web Storage conformance or a measured page-load speedup.

## Contract

- [HTML Storage](https://html.spec.whatwg.org/multipage/webstorage.html#the-storage-interface)
  defines one string map behind methods and named properties. Get/set/delete, enumeration, and
  property descriptors share that map, with prototype members remaining visible. Missing
  arguments, throwing conversions, and Symbol-to-DOMString conversions follow Web IDL.
- [DOMString](https://webidl.spec.whatwg.org/#idl-DOMString) preserves UTF-16 code units, including
  NUL and isolated surrogates. `StorageString` shares immutable code units between the map,
  optimistic intents, and snapshots. Storage never round-trips through lossy UTF-8.
- The implementation-defined origin budget is **5 MiB / 1,024 entries** per area. Accounting
  preserves the previous UTF-8 byte cost for valid strings; an isolated surrogate costs three
  quota bytes. Combined key/value size and replacement delta are checked before mutation.
  No-op writes consume neither versions nor pending-write capacity.
- Synchronous quota failures throw Web IDL's
  [QuotaExceededError](https://webidl.spec.whatwg.org/#quotaexceedederror), a DOMException subclass
  with legacy code 22 and nullable `quota`/`requested` fields. Its constructor, options, and
  structured-clone representation are implemented; storage failures leave optional values null.
- `StorageEvent` supports branded read-only payload accessors, dictionary conversions,
  `initStorageEvent`, `document.createEvent("StorageEvent")`, and Window `onstorage`.
  Nullable keys/values retain exact UTF-16; `url` converts to USVString without resolving it.

## Synchronization and event delivery

[HTML's broadcast algorithm](https://html.spec.whatwg.org/multipage/webstorage.html#the-storage-interface)
selects other same-origin local-storage holders and queues a DOM-manipulation task per recipient.
The implementation for [issue #149](https://github.com/VictorZakharov/better-web-browser/issues/149)
uses these boundaries:

1. A browser-owned `StorageCoordinator` atomically subscribes a document and captures its initial
   snapshot. Membership is document-scoped and removed on navigation, close, or renderer failure.
2. Each renderer keeps an authoritative base map plus an ordered journal of synchronous local
   intents. Per-source sequence acknowledgements are independent of the shared origin version.
   Incoming changes rebase beneath pending writes instead of replacing or erasing them.
3. The authority validates the source document's origin, mutation-time URL, and sequence, then
   applies adjacent operations against the latest map. Web Storage is not compare-and-swap:
   concurrent callers with old cached versions may still write. Every actual operation retains
   its own old/new values, even when a batch's final map is unchanged.
4. Durable commit precedes publication. The source receives acknowledgements, not `storage`
   events. Other same-origin tabs receive ordered changes. Identical sets, missing removes,
   empty clears, quota rejection, and failed persistence produce no foreign events.
5. A recipient applies the map update before queuing its event. The event is trusted,
   non-bubbling, non-cancelable, and non-composed, with the recipient's `Storage` object and the
   writer's mutation-time URL. Dispatch occurs in a later document task with a microtask
   checkpoint; handlers may write reentrantly. The native dispatch callback is captured privately
   and removed from the page global, so page code cannot use it to manufacture trusted events.
6. Each recipient admits one IPC update at a time. Its slot remains occupied until a local
   acknowledgement is applied or a foreign event task finishes. Cookie/snapshot corrections
   can coalesce; storage changes cannot. Event-task progress remains unblocked while waiting
   for its receipt. Late receipts from a replaced document cannot fail its successor.

`sessionStorage` remains private to a top-level tab and survives same-tab navigation. HTML allows
session-storage broadcasts between same-origin documents in the **same** top-level traversable;
this change does not add executable child-frame documents or claim that iframe scope is complete.
Background top-level tabs are still eligible recipients: hidden is not the same as inactive.

## Bounds and durability

Sets stop at **32 MiB / 4,096 unacknowledged intents per area**. This counts already-transmitted
writes until their receipts, not just the latest outgoing drain. Cleanup remains available after
exhaustion. UTF-16 buffers may occupy up to twice the quota-byte count.

Each browser recipient has a **32 MiB / 4,096-record** queue, including old values, new values,
keys, and source URLs. A full recipient defers the next transaction **before commit**. The UI
retains that transaction and its FIFO barriers until retry; it does not drop records. Closing the
recipient releases its queue. The ordinary renderer event queue also retains byte backpressure.

The normal UI drain remains 32 events; a trailing storage run may extend to 256 intents / 1 MiB
using only already-queued entries. Adjacent same-document, same-area operations are batched;
large transactions split at 8 MiB so their old/new records fit an empty recipient queue. One
local-storage batch performs one file rotation/flush. Failure restores the map and version under
the transaction lock, sends rejection acknowledgements, and records an actionable browser status.

JavaScript sees an optimistic synchronous projection: successful `setItem` is not a promise that
disk I/O has completed. A concurrent quota conflict or disk failure is repaired asynchronously by
retiring the rejected intents, preserving unrelated pending writes. This durability boundary is
explicit; it is not full synchronous cross-process linearizability.

IPC major **10** uses length-prefixed little-endian UTF-16 for storage. Ordinary controls retain
256 KiB limits and other bulk frames retain 8 MiB limits. Separate per-kind ceilings cover:

| Payload | Maximum encoded bytes |
|---|---:|
| Initial storage entry | 10 MiB + 26 |
| Mutation with source sequence and URL | Entry limit + `MAX_URL_BYTES` + 12 |
| Synchronization record with full old/new values | 20 MiB + `MAX_URL_BYTES` + 64 |

The reader rejects over-budget headers before allocation, validates lengths before copying, and
revalidates combined key/value quota. The larger storage ceiling does not increase other frame
limits. No new dependencies or site-specific rules are involved.

Disk format **2** keeps ordinary JSON strings and uses code-unit arrays only for isolated
surrogates. Format 1 remains readable, including origins larger than 2.5 MiB; successful writes
migrate it. Serialization/reads retain the 64 MiB file budget and temporary-file/backup recovery.
Session storage is never written to disk. Older executables cannot read format 2; downgrading
requires a compatible profile backup.

## Verification

The original `benchmarks/alpha/fixtures/storage-sync.html` runs unchanged in two real isolated
Breeze renderer processes and two tabs in a fresh, muted, unified-headless Chrome process.
The eight checks cover ordered exact-string payloads, trusted event flags/target, recipient
`storageArea`, writer URL, no-ops, session isolation, source no-echo, and reentrant reply writes.
The Chrome screenshot's visible PASS result was inspected. This is behavioral evidence, not a
native two-tab UI screenshot comparison or a performance benchmark.

| Check | Before #149 | Breeze | Headless Chrome 152.0.7977.83 |
|---|---:|---:|---:|
| Original cross-tab fixture | Missing | 8/8 | 8/8 |
| Original event-interface fixture | 0/26 | 26/26 | 25/26 (legacy target reset differs) |
| Unchanged upstream event constructor / initializer | Not selected | 2 files / 11 assertions | Not run |

The event-interface fixture deliberately retains DOM's
[event-initialization](https://dom.spec.whatwg.org/#concept-event-initialize) requirement to reset
`target` to null. Chrome retains the old target after legacy initialization; that difference is
recorded rather than removed from the test.

Owned tests cover concurrent pending intents, origin/port boundaries, session isolation, no-ops,
count/byte backpressure, close/re-subscribe, sequence replay/gaps, quota conflicts, disk failure,
trusted dispatch despite page overrides, microtasks, and cancellation. Real hidden renderer tests
also exercise reentrant replies, navigation racing an admitted update, and a record carrying two
full-quota old/new values through the pipe without truncation.

The existing value fixture (`web-storage-values.html`) passed 10/10 in both engines and its Breeze
restart mode passed 3/3 in the preceding slice. That slice also verified Wikipedia ResourceLoader's
1,063,405-code-unit cache value and a readable Coron climate table without storage warnings.
The curated upstream gate contains 217 files / 2,137 assertions, including 25 Web Storage files;
these are unchanged upstream tests from the pinned checkout, not a whole-spec pass rate.
Iframe/window-opening broadcast WPTs require browsing-context capabilities outside this slice;
they are not presented as passing coverage.

The first #149 CI run exposed the two quota-independence stress files exceeding the existing
two-second JavaScript watchdog under contention. The same failure reproduced locally with the
full debug WPT suite, eight jobs, and the test process/children restricted to two logical CPUs.
Profiling identified costly scalar decoding for quota accounting and quadratic replay of pending
intents on otherwise neutral source acknowledgements. Direct UTF-16 quota counting preserves
the old byte cost (checked against scalar decoding across all code units and surrogate boundaries).
Acknowledging an identical oldest operation now advances the base without rebuilding an unchanged
visible map; foreign changes and rejected operations still rebase.

In local debug measurements, the representative script changed from 364 ms to 120 ms; draining
a 3,002-intent journal changed from 5,929 ms to 62 ms. The constrained full WPT rerun passed all
217 files / 2,137 assertions, with the formerly timed-out script at 427 ms instead of 2,072 ms.
These are single diagnostic runs, not navigation benchmarks. Watchdog limits, WPT deadlines,
parallelism, storage quotas, and upstream assertions were unchanged.

## Remaining boundaries

Synchronization between separate browser application instances, executable iframe broadcast
scope, third-party partitioning, and full policy/opaque-origin access-denial behavior remain
follow-up work. This change does not implement them by exposing a synthetic iframe shim.
