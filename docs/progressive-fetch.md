# Progressive Fetch response delivery

Issue [#150](https://github.com/VictorZakharov/better-web-browser/issues/150) covers response streams
in documents and dedicated workers, not general navigation speed. This implementation uses the
[Fetch response processing model](https://fetch.spec.whatwg.org/#fetch-method) and
[ReadableStream default tee algorithm](https://streams.spec.whatwg.org/#readable-stream-default-tee).
No site-specific request rules or new dependencies are involved.

## Delivery and ownership

The browser applies the existing request, credentials, CORS, redirect, response-header, null-body,
and size policies. Typed response heads resolve `fetch()` without awaiting the body; chunks become
fresh `Uint8Array` values; EOF closes the stream. A pre-header failure rejects Fetch; a subsequent
failure errors the body. Existing locking, disturbance, Body mixin, and immutable network-header
rules remain in force. Aborting preserves the signal's exact reason for pending readers, including
both clone branches.

Dedicated workers now receive asynchronous head/chunk/end/error commands. Entry scripts and
`importScripts()` remain buffered, but their waits observe termination rather than keeping a dead
worker alive until its server responds. Worker IDs and script request IDs map to distinct wire
request IDs. Navigation, cancellation, and termination retire those mappings; stale deliveries
cannot enter the replacement document or another worker.

## Backpressure and limits

The default stream queue accounts for bytes with `strategy.size`, not just chunk count. A dequeue
produces a monotonic consumed-byte receipt. Protocol major 11 adds the document/request-scoped
`FetchResponseConsumed` frame; the broker rejects backwards or over-reserved credits and ignores
receipts racing a terminal event for a retired request.

| Boundary | Limit / behavior |
|---|---|
| Response chunk | 64 KiB maximum |
| Active script response | 256 KiB unconsumed delivered bytes |
| Renderer session | 4 MiB aggregate in-flight delivery credit |
| Network scheduling | At most 8 workers per batch; idle bodies park and yield slots to other requests |
| Producer read-ahead | Capacity checked before reading; a racing credit reservation retains at most one chunk per response until retry |
| Script response total | 64 MiB; EOF does not remove application-retained data |
| Buffered response total | Existing 16 MiB consumer limit remains |

The existing eight-chunk IPC queue remains bounded. Only producers can wait on that queue; neither
the UI nor the document/worker event loop waits for a body. Parked response jobs are revisited after
5 ms, without adding a thread per response. Abort/navigation/exit release outstanding credit.

`Response.clone()` tees on demand: two idle branches do not eagerly drain a response. As required
by Streams, the faster branch can cause the slower branch to retain data. Therefore the credit
window is **not** a global JavaScript heap cap. Full-body readers and retained clone branches can
hold up to the response limit. Byte streams/BYOB, streaming uploads, and complete WritableStream,
TransformStream, and pipe algorithms are not claimed by this slice.

## Reproduce the comparison

In one terminal, run the owned loopback fixture server and retain the printed URL:

```powershell
./scripts/serve-progressive-fetch.ps1 -ReadyFile target/progressive-fetch/server.txt
```

In another terminal, substitute that URL's port:

```powershell
./scripts/run-hidden-benchmark.ps1 -Url http://127.0.0.1:PORT/progressive-fetch.html `
  -Output target/progressive-fetch/breeze.json -Screenshot target/progressive-fetch/breeze.png `
  -FreshProfile -SettleMs 6500 -WindowWidth 1250 -WindowHeight 1050
dotnet run --project benchmarks/chromium/ChromiumBaseline.csproj -c Release -- `
  --url http://127.0.0.1:PORT/progressive-fetch.html --output target/progressive-fetch/chrome.json `
  --screenshot target/progressive-fetch/chrome.png --viewport-width 1250 --viewport-height 1050 `
  --settle-ms 6500 --require-fixture-ready
```

Both harnesses are hidden/headless and silent. Stop the owned server with Ctrl+C. The server sends
a first chunk after 300 ms and a final chunk 1.5 s later. Its phase endpoint checks progress before
EOF; the page tests header availability, first chunk, timer responsiveness, and clone bytes in both
realms. HTTP chunking is deliberate: HttpListener otherwise buffers the fixture's body until close.
The visible PASS/FAIL summary and console results are the acceptance evidence, not command exit 0.

## Regression coverage

### Measured comparison, 2026-09-15

One recorded run per build against the same owned delayed-chunk fixture (fresh profiles, hidden
release Breeze and muted headless Chrome 152.0.7977.83). Times are milliseconds from each realm's
Fetch call, not navigation timings or statistically estimated speed ratios.

| Observation | Before (PR #151 release) | This release | Chrome |
|---|---:|---:|---:|
| Fixture assertions passing | 5 / 8 | 8 / 8 | 8 / 8 |
| Document first chunk | 335 ms | 345 ms | 854 ms |
| Worker response headers | 1,863 ms | 339 ms | 843 ms |
| Worker first chunk | 1,875 ms | 342 ms | 844 ms |
| Worker completion | 1,885 ms | 1,854 ms | 1,845 ms |
| Worker headers/chunk/timer before server EOF | No / No / No | Yes / Yes / Yes | Yes / Yes / Yes |

The previous documentation overstated the gap: documents were already progressive; the worker
path waited for completion. The meaningful change is earlier usable worker data and bounded
cross-process flow control, not faster network completion. Screenshots and reports are local
validation artifacts under `target/progressive-fetch/`; they are not vendored source assets.

### Automated gates

- Hidden renderer tests separately withhold chunks and EOF, exercise exact abort reasons through
  clones, cancel entry loads and worker bodies, block an idle producer, and navigate while blocked.
- The hidden live-network test starts 17 requests per realm (34 total), waits for every response
  head before reading any body, then consumes every original and clone. This detects network-slot
  starvation that a single-response test misses. Existing concurrent 25 MiB XHR/abort/retry and its
  128 MiB renderer working-set ceiling remain gated.
- Unchanged pinned WPT `tee.any.js` and `floating-point-total-queue-size.any.js` add 30 upstream
  assertions, including error microtask ordering, cancellation, demand-driven pulls, and queue
  arithmetic. The WPT wrapper runs Window variants; actual Worker coverage is in the owned tests.

This improves API availability and prevents buffering stalls. It is not evidence that Wikipedia or
YouTube navigation is faster; those require their own visual loading measurements.
