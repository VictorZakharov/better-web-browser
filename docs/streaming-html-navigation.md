# Streaming HTML navigation

Implemented 2026-09-15. This is a bounded navigation slice, not full HTML conformance,
whole-site parity, or a general page-load speedup claim.

## Contract and ownership

The browser submits response headers and a replayable main-response stream to the
isolated renderer. The parser no longer waits for the complete response body.
Incoming chunks wake the broker; bounded IPC batches leave room for input and
subresource replies. Exhausted input suspends the retained tokenizer/tree builder
without pretending it reached EOF. Prefix CSS, script preparation/execution,
timers and rendering can progress while the network is still downloading.

The existing `encoding_rs` decoder retains incomplete multibyte sequences. A BOM
can cross chunks and overrides the transport charset; a transport label is certain.
Otherwise the implementation defaults tentatively to UTF-8. Encoding declarations
come from html5ever's actual tree builder, not text matching in comments/scripts.
An effective declaration changes confidence and, when necessary, replays retained
bytes in a fresh document/realm without downloading again. HTML's UTF-16 and
`x-user-defined` normalization rules apply. Storage projections and pending write
sequence journals survive; abandoned fetches are retired and late replies cannot
enter the replacement. Presentation revisions remain monotonic.

References: HTML's [input byte stream and encoding determination](https://html.spec.whatwg.org/multipage/parsing.html#the-input-byte-stream),
[changing the encoding](https://html.spec.whatwg.org/multipage/parsing.html#changing-the-encoding-while-parsing),
and [end of parsing](https://html.spec.whatwg.org/multipage/parsing.html#the-end).
No new dependencies or site-specific branches are introduced.

`DOMContentLoaded` and window `load` retain their separate parser/script/resource
gates. Network EOF alone does not force a layout if it changed no DOM state.
The loading indicator remains active until the main response ends even when a
prefix has painted. The first-presentation timeout starts after network completion,
so a slow response does not consume the renderer's recovery allowance. Renderer
health monitoring remains active throughout. A failed body becomes an actionable
document failure, not a silently successful partial load; later navigation remains usable.

Main-response bytes remain bounded by `MAX_RESPONSE_BODY_BYTES`; chunks by
`MAX_FETCH_STREAM_CHUNK_BYTES`, decoded input by `MAX_HTML_INPUT_BYTES`, and DOM
construction by existing node/depth ceilings. The replay buffer is intentionally
different from script Fetch's consumption-credit stream. Renderer retries can read
the saved prefix. Decoder/source replay storage is released after parser EOF;
pending scripts cannot grow input without these limits.

## Verification and reproduction

Owned tests exercise fragmented tags/entities/raw text, incomplete scripts,
UTF-8/UTF-16 boundaries, BOM/transport precedence, charset replay, pending storage
writes, response failure, cancellation, stale fetch replies, stream limits, and
slow-response deadlines. A hidden live-runtime fixture tests the production
network-to-renderer path and demands an external request before the response tail.
Green unit tests alone are not visual acceptance.

The end-to-end withheld-tail fixture uses HTTP/1.1 chunked transfer. The separate
small `Content-Length` transport test compares against a raw TCP control and always
checks byte integrity. On this Windows host both raw TCP and WinHTTP withheld the
192-byte body until all eight server writes completed. If the socket control does
that, the test explicitly reports its latency check as unavailable; a passing test
is not evidence of early delivery for that response shape. When the raw control
streams, WinHTTP must stream too. No host security settings were changed. This
external buffering boundary remains unresolved, not a claimed navigation speedup.

Start `scripts/serve-streaming-navigation.ps1 -ReadyFile target/streaming-navigation/server.txt`.
It serves the owned `benchmarks/alpha/fixtures/streaming-navigation.html` with a
2.5-second withheld tail; each navigation has its own resource timing token.
The successful title is `Streaming navigation PASS`; all five checks must be true.

Run Breeze through `scripts/run-hidden-benchmark.ps1` with `-FreshProfile`,
`-SettleMs 3500`, `-FilmstripIntervalMs 500`, `-FilmstripDurationMs 4500`,
`-WindowWidth 1520`, `-WindowHeight 1000`, and selectors `#prefix`, `#tail`, `#result`.
Use `-Browser` for a saved merged baseline. Run the existing Chromium harness
headless and muted, with matching viewport 1506 × 828 and device scale 1.25.
Keep reports/profiles/screenshots in ignored output directories.

Chromium's filmstrip samples its latest compositor-delivered screencast frame on
the requested wall-clock grid. The manifest identifies this source and records
both the sample time and source-frame time; unchanged frames repeat. The initial
blank surface is captured before navigation. This avoids a reproducible Windows
Chrome hang when `Page.captureScreenshot` is requested before first paint, without
forcing layout or waiting for page load. CDP responses and events share one
multiplexed connection. Failed captures are not valid comparison evidence.
See the official [Page screenshot/screencast protocol](https://chromedevtools.github.io/devtools-protocol/tot/Page/).

`page_ready_ms` is Breeze's first presentation, whereas Chromium's similarly named
field is its load-event observation. Do not divide those fields to claim a speedup.
Use the filmstrips and the fixture contract. Breeze's `network_ms` is `null` if a
capture ends before network completion, rather than mislabeling header latency.

## Release evidence (2026-09-15)

One sequential, fresh-profile run per browser of the owned 2.5-second withheld-tail
fixture; screenshots are sampled every 500 ms. These are observations, not a
statistical benchmark or a speed ratio between different readiness metrics.

| Observation | Merged Breeze (PR #152) | Streaming Breeze | Headless Chrome 152 |
| --- | --- | --- | --- |
| Inspected 500 ms sample | Blank page | Styled prefix, waiting for tail | Blank surface |
| Inspected 1000 ms Chrome sample | — | — | Styled prefix, waiting for tail |
| First presentation (Breeze metric) | 2599 ms | 241 ms | Not the same metric |
| Main-response completion (Breeze metric) | 2577 ms | 2547 ms | Not collected by this harness |
| Final owned fixture contract | FAIL: cannot progress before tail | PASS, all five checks | PASS, all five checks |

Chrome reported first paint at 576 ms and its navigation load event at 2544 ms.
The visual result demonstrates removal of Breeze's whole-main-response gate, not
faster completion of the network transfer.

Final hidden captures of Wikipedia Main Page, Coron, Palawan, and 2026 Yemen
offensives were compared with headless Chrome. Their inspected first-content
samples were styled; final captures had readable article/panel/infobox layouts,
zero reported Breeze JavaScript errors, and no renderer exits. Images can still
arrive after text. Spacing, native controls, and image sizing are not pixel-identical
to Chrome; Main Page spacing was also present in the merged baseline. These checks
are startup smoke tests, not acceptance of every Wikipedia interaction.

Local gates: 1376 Rust tests passed (4 intentionally ignored live probes), strict
Clippy, format/source-size checks, Chromium harness self-tests including compositor
filmstrip delivery, and 219 curated WPT cases / 2167 subtests passed with no failures
or timeouts. The small Content-Length latency limitation above remains explicit.

## Remaining standards work

Synchronous re-entrant `document.write()` and complete stylesheet-set selection
remain separate from the implemented [import dependency loading](stylesheet-loading-dependencies.md).
Event-handler content attributes, parser custom-element reactions at every token,
and nested browsing-context loading remain separate standards slices. This change
does not claim to resolve the existing YouTube seeking or layout issues, nor does
it claim a CPU/memory reduction or Wikipedia loading within a fixed Chrome margin.
