# Performance Timeline and User Timing

## Implemented contract

Window and dedicated Worker realms share the same User Timing and observer implementation.
This replaces the former writable entry records and Date-based clock; it is not a no-op
`PerformanceObserver` compatibility shim.

| Surface | Behavior |
|---|---|
| `PerformanceObserver` | Per-observer buffer, separate performance-timeline task source, delivery after a microtask checkpoint, callback exception reporting |
| Observation | `type`/`buffered` and `entryTypes` modes, reconfiguration, repeated buffered retrieval, case-sensitive filtering, immutable supported-types list |
| Lifetime | `takeRecords()` drains that observer; `disconnect()` clears registration and pending records while preserving the single/multiple mode |
| Entries | Read-only branded fields; sorted snapshot retrieval; clearing the performance timeline does not erase queued observer records |
| User Timing 3 | Mark constructor, mark/measure return values, structured-cloned details, named marks, dictionary conversion, timestamp validation and valid negative derived values |
| Clock | Native monotonic clock, shared process epoch estimate across Window/Workers, 100-microsecond coarsening of both endpoints |

The task is queued through a private host hook, not author-replaceable `setTimeout`.
Guessed `clearTimeout` IDs cannot cancel it. Mark creation uses the internal clock and
clone implementation even if a page replaces `performance.now`, `Date.now`, or
`structuredClone`. Observer exceptions are reported without aborting later observers.

User Timing entries remain in the realm timeline until explicitly cleared. Observer buffers
are independent of that timeline and of each other. Marks and measures do not use the Resource
Timing buffer cap, so their first delivery after `observe()` reports zero dropped entries;
that is not a claim that unimplemented resource observations succeeded.

The implementation follows [Performance Timeline](https://www.w3.org/TR/performance-timeline/),
[User Timing Level 3](https://www.w3.org/TR/user-timing/), and
[High Resolution Time](https://www.w3.org/TR/hr-time-3/). The host scheduler shares a deterministic
FIFO queue across task sources; it does not yet add a low-priority delivery heuristic.

## Validation

The pinned, unchanged upstream selection adds 39 files / 121 assertions covering observer
delivery, buffered replay, mutation inside callbacks, disconnect/reconfiguration, record retrieval,
User Timing constructors and dictionaries, structured serialization, and clock monotonicity.
The complete curated gate is 192 files / 886 passing assertions with no expected failures.
The runner now loads upstream META script dependencies instead of silently omitting helpers.

Owned tests exercise private scheduling under author overrides, exception isolation, reentrant
delivery, immutable entry fields, single-read dictionary iteration, subtype branding, rejected
clones, and actual dedicated Worker task delivery. These complement the upstream Window runs;
the curated runner does not claim to execute `.any.js` Worker variants.

The original [cross-browser fixture](../benchmarks/alpha/fixtures/performance-observer.html)
checks eight contracts in hidden Breeze and muted unified-headless Chrome. Both report
`script,microtask,observer`. Serve it with `scripts/serve-alpha-fixtures.ps1` and select `#result`
for machine-readable `data-status`/`data-sequence` diagnostics.

The live Coron, Palawan Wikipedia run no longer reports the formerly caught
`PerformanceObserver is not defined` exception. ResourceLoader's unrelated `invalid storage
value` warning remains. This feature is not a page-load speedup measurement or a claim of
pixel-identical rendering.

## Explicit boundaries

- `supportedEntryTypes` advertises only `mark` and `measure`. Resource, Navigation, Paint,
  Long Task, Event Timing and other producers remain unimplemented and are not advertised.
- Legacy `performance.timing` is still a compatibility surface: only the realm origin is known;
  unavailable milestones remain zero. Resource buffer methods do not create resource entries.
- Window time origin is currently captured when its script host is created, not at the start
  of browser navigation. Navigation-start propagation and legacy navigation milestones are
  separate work. Communicating Window/Worker realms in one process share an epoch estimate;
  cross-process clock-origin synchronization and independent iframe realm lifecycle are not
  established by this slice.
- Cross-origin-isolated higher-resolution clocks and newer tentative entry identity/navigation
  attributes are outside this implementation.
- Upstream `performance-timeline/not-clonable.html` requires an actual Navigation Timing entry;
  `performance-timeline/case-sensitivity.any.js` also requires Resource Timing entries; and
  `hr-time/performance-tojson.html` includes legacy PerformanceNavigation. They are not in this
  focused all-pass selection. Owned tests do verify mark clone rejection and case-sensitive
  User Timing tests remain unchanged. No placeholder entries were added to satisfy those tests.

These boundaries prevent the curated result from being mistaken for full Performance Timeline,
Navigation Timing, or whole-platform conformance.
