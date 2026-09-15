# Web Storage values, quotas, persistence, and events

This implements the value/mutation contract for `localStorage` and `sessionStorage`, plus the
`StorageEvent` interface foundation described below. Cross-tab broadcasting is not yet connected.
It is not a claim of full Web Storage conformance or a measured page-load speedup.

## Contract and implementation

- [HTML Storage](https://html.spec.whatwg.org/multipage/webstorage.html#the-storage-interface)
  defines one string map behind methods and named properties. Breeze now routes get/set/delete,
  enumeration, and property descriptors through that map, with prototype members remaining visible.
  Missing arguments, throwing conversions, and Symbol-to-DOMString conversions follow Web IDL.
- [DOMString](https://webidl.spec.whatwg.org/#idl-DOMString) preserves UTF-16 code units, including
  NUL and isolated surrogates. `StorageString` shares immutable code units between the map,
  optimistic intents, and snapshots. The V8 bridge and storage wire codec never round-trip through
  a lossy Rust UTF-8 string.
- The implementation-defined origin budget remains **5 MiB / 1,024 entries** per area. The old
  4 KiB key and 192 KiB value caps are gone. Quota accounting retains the previous UTF-8 byte cost
  for valid strings, so old profiles do not become over-quota merely by migrating; an isolated
  surrogate costs three quota bytes. Combined key/value size and replacement delta are checked
  before mutation. No-op writes do not consume versions or pending-write capacity.
- Rejected writes throw the current Web IDL
  [QuotaExceededError](https://webidl.spec.whatwg.org/#quotaexceedederror), a DOMException subclass
  with legacy code 22 and nullable `quota`/`requested` fields. The constructor, options, and
  structured-clone representation are implemented; storage failures leave both optional values null.

## Transport and durability

IPC major **9** uses length-prefixed little-endian UTF-16 for storage only. Ordinary control
messages retain their 256 KiB limit and other bulk frames retain their 8 MiB limit. The two storage
payload kinds have a separate **10 MiB + 26 byte** ceiling: worst-case UTF-16 representation of a
5 MiB ASCII map entry plus fixed metadata. The reader rejects over-budget headers before payload
allocation, validates string lengths before copying, and revalidates combined origin quota.

Script-originated pending sets stop at 32 MiB of quota bytes or 4,096 intents per drain. Cleanup
remains possible after exhaustion; deleting both projected areas can add at most another 10 MiB
of key bytes. The broker applies byte-based backpressure at 32 MiB instead of dropping storage
events. UTF-16 native buffers can use up to twice the quota-byte count. None of these limits
change the origin's persistent quota, and no new dependency or site-specific rule is involved.
Trailing presentations and diagnostics also wait for a queue slot after coalescing, preserving
FIFO barriers when storage fills the event queue. The nonblocking teardown path stays nonblocking.

Adjacent same-document, same-area intents are committed together. The normal UI drain remains
32 events; a trailing storage transaction can extend to 256 intents / 1 MiB of quota bytes using
only already-queued entries. The front event is checked before removal, so the extension never
consumes another document, area, or event kind. Large entries already in the normal drain do not
qualify for an extension. Every version is checked in order. One
localStorage batch produces one file rotation/flush. A failed batch restores the previous map and
version while still holding the transaction lock. The renderer's optimistic map is repaired through
the existing authoritative snapshot path if browser-side persistence rejects an intent.

Disk format **2** retains ordinary JSON strings and uses code-unit arrays only for strings containing
isolated surrogates. Format 1 remains readable, including origins larger than 2.5 MiB; successful
writes migrate it. Serialization and input reads are bounded by the existing 64 MiB file budget.
Temporary-file/backup recovery remains in place. Session storage is never written to disk.
Older executables cannot read format 2; downgrading requires a compatible profile backup.

## Verification

The original fixture is `benchmarks/alpha/fixtures/web-storage-values.html`. Its normal mode writes
values; `?read` verifies them after a new browser process opens the same isolated profile.

| Check | Previous release | This implementation | Headless Chrome |
|---|---:|---:|---:|
| Value/API fixture | 2/10 | 10/10 | 10/10 |
| Restart persistence fixture | Not run | 3/3 | Not run |

Both screenshot captures were inspected. They verify the visible pass results, not pixel parity:
Breeze's pre-existing ordered-list marker rendering still differs from Chrome.

The live Coron, Palawan Wikipedia check persisted ResourceLoader's **1,063,405-code-unit** cache
value, which previously exceeded the 192 KiB item cap, without the `invalid storage value` warning.
The run reported no JavaScript errors. This is cache compatibility evidence, not a loading-time claim.

The curated gate adds 23 unchanged upstream Web Storage files from the existing pinned WPT checkout.
Large callback reports are transported in bounded, ordered chunks without omitting subtests;
missing or malformed chunks fail the run. Each WPT case now receives a separate temporary profile,
instead of sharing the default browser profile. The upstream tests and timeout limits are unchanged.

All 23 files pass, covering 1,240 upstream assertions. With isolated profiles held constant, the
two quota-independence stress files changed from timeouts (14.15 / 20.05 seconds) to passing
in 2.81 / 3.18 seconds after batching. These are single local test runs, not navigation benchmarks.
At completion of the value/persistence slice, the curated gate passed 215 files / 2,126 assertions, including a debug-build run with
eight parallel jobs. In that debug configuration, extending the bounded adjacent batch reduced
the quota-independence cases from 7.57 / 5.77 seconds to 2.36 / 2.32 seconds in local single runs;
the upstream tests and deadlines were not changed. A second Wikipedia process reused
the saved profile and activated the Climate anchor without JavaScript errors or a storage warning;
its table screenshot remained readable without overlapping rows.

Owned tests cover full-quota items, wire round trips, oversized/truncated frames, real hidden renderer
delivery, queue backpressure/teardown, rejected replacements, failed disk writes, batched commit and
rollback, version-1 migration, restart persistence, and quota exception cloning.

## Remaining boundaries

Cross-document `storage` event broadcasting, synchronization between separate browser processes,
third-party partitioning, and policy/opaque-origin access-denial behavior remain follow-up work.
JavaScript observes an optimistic synchronous projection: a returned `setItem` is not a promise that
disk I/O has already completed. Browser-side failure repairs that projection asynchronously.
The selected WPT files are not a whole-spec pass rate.

## Cross-tab synchronization follow-up (in progress)

[Issue #149](https://github.com/VictorZakharov/better-web-browser/issues/149) tracks same-origin
tab synchronization and queued `storage` events. The initial implementation adds:

- `StorageEvent` construction, read-only branded payload accessors, Web IDL dictionary conversion,
  legacy `initStorageEvent`, `document.createEvent("StorageEvent")`, and the Window `onstorage`
  event-handler attribute. `key`, `oldValue`, and `newValue` retain exact UTF-16 code units; `url`
  converts to a USVString without resolving a relative string.
- `LocalStorage::apply_batch_with_changes`, which records each actual operation under the same
  lock as the mutation and durable commit. Repeated changes within a batch remain separate;
  no-op operations and rejected transactions expose no records. The existing boolean-only
  application path does not allocate records. This API is not yet connected to tab broadcasting.

The next step is separating a renderer's pending-write acknowledgements from the shared origin's
version, followed by bounded recipient routing and DOM-manipulation task delivery. A snapshot
replacement alone would erase pending optimistic writes. Cross-tab events, navigation/closed-tab
delivery tests, and session-storage scope coverage are **not complete**. The `StorageEvent`
interface's presence must not be used as evidence that tabs are already synchronized.

The original `benchmarks/alpha/fixtures/storage-event-interface.html` tests only construction and
synthetic dispatch. It deliberately retains the DOM
[event-initialization](https://dom.spec.whatwg.org/#concept-event-initialize) assertion that legacy
initialization resets `target` to null. The headless Chrome comparison retains the old target,
matching its current [Event::initEvent implementation](https://github.com/chromium/chromium/blob/main/third_party/blink/renderer/core/dom/events/event.cc).
This difference is recorded rather than excluded from the fixture.

| Initial event-interface check | Previous release | Current branch | Headless Chrome 152.0.7977.83 |
|---|---:|---:|---:|
| Original construction/dispatch fixture | 0/26 | 26/26 | 25/26 (target reset differs) |
| Upstream event constructor / legacy initializer | Not selected | 2 files / 11 assertions pass | Not run |
| Cross-tab synchronization / trusted event delivery | Missing | Not connected | Not tested in this slice |

The curated release gate now passes 217 files / 2,137 assertions with no expected failures.
Six owned mutation-record tests cover intermediate values, no-op suppression, exact strings,
quota/stale-version rollback, failed persistence, and origin/area isolation. No page-load
performance claim is made for this interface foundation.
