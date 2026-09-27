# WebSocket, IndexedDB, and temporal form-control slice

This slice implements browser behavior behind feature probes; it does not
special-case HTML5test. The normative references are the
[WebSockets Standard](https://websockets.spec.whatwg.org/),
[Indexed Database API](https://w3c.github.io/IndexedDB/), and
[HTML input-state algorithms](https://html.spec.whatwg.org/multipage/input.html).
The implementation remains a bounded technical alpha, not complete
conformance to those specifications.

## WebSocket ownership

- Document and dedicated-worker realms have the WebSocket constructor, URL/protocol validation,
  open/message/error/close events, text and binary sends, binaryType, buffered
  amount, and close-code handling. The page cannot open a socket directly.
- The renderer sends a bounded command through its document broker. The
  browser resolves the initiating page or committed worker client and its
  origin, applies that client's CSP connect-src and mixed-content policy, adds
  relevant cookies, and owns the WinHTTP connection and frame transport.
  A worker uses its entry response's CSP, not its creator page's connect-src.
- Worker sockets use a disjoint wire-ID namespace. Replies return only to the
  initiating worker; late events after termination and late frames after a
  retired navigation cannot reach the page or a new realm. Termination sends
  a browser-side cancellation for active and in-flight sockets. Queues and
  payloads have explicit ceilings rather than unbounded renderer-to-browser
  memory use. At most eight sockets are admitted per document and 32 socket
  pairs per browser process. Each established pair uses a reader and writer
  thread, so this bounds socket-owned native threads to 64; canceled handshakes
  keep their reservation until their threads exit.
- Hidden integration fixtures check page and worker loopback handshakes,
  Origin and subprotocol headers, text/binary frames, clean closes, Worker
  response CSP, and in-flight handshake teardown on termination/navigation.

This is a baseline WebSocket implementation. Extension negotiation,
compression, and broader network interoperability remain work to do. WinHTTP
currently opens sockets synchronously: cancellation revokes the realm and
suppresses late events immediately, but a blocked native handshake can occupy
its worker thread until WinHTTP returns or times out; forcibly closing that
synchronous request from another thread is unsafe. Absent features must not be
hidden behind a site-specific fallback.

## IndexedDB ownership and behavior

- Profile persistence belongs to the browser process. The renderer can only
  request operations for a browser-resolved document, frame, or committed
  dedicated-worker client. Script cannot choose another origin or a profile path.
- Databases are partitioned by tuple origin. Opaque origins fail rather than
  sharing a global namespace. The browser bounds both individual IPC messages
  and on-disk origin/database size.
- The exposed window and dedicated-worker APIs cover asynchronous open,
  upgrade, delete, database listing, object-store creation/deletion, read-only
  and readwrite transactions, structured-cloned values, generated keys, key
  paths, get, getKey, put, add, delete, clear, count, getAll, getAllKeys, key
  ranges, and forward/reverse object-store cursors.
- IndexedDB keys have their own type order: number, date, string, binary,
  array. NaN, infinities, malformed ranges, and excessive nesting fail at
  both the page and browser boundaries. Generated inline keys are injected
  into the persisted structured clone rather than merely returned to script.
- A readwrite transaction stages its writes in the browser. Request callbacks
  can enqueue more work and observe earlier staged writes. A commit writes
  the candidate state atomically; abort or a failed request discards it. A
  generation check prevents an older concurrent session from overwriting a
  newer committed database.
- A cursor advancement asks for one ordered record, not a whole-store dump.
  The browser's result ceiling still applies to every returned record.
- Persistence uses a recoverable profile file. Tests check origin isolation,
  ordering, rollback, conflicting commits, malformed keys, nested generated
  keys, and reopening the same profile in a second hidden browser process.
- Worker requests use the same browser-owned service and tuple-origin store as
  their page. Replies return to the initiating worker's event loop; late replies
  after worker termination cannot invoke a page callback. A hidden integration
  fixture verifies worker write → page read/write → worker read. Another hidden
  fixture terminates two Workers holding 64 successful browser Steps before
  Commit, verifying rollback and session-capacity recovery. Opaque clients
  are rejected using their committed origin, even if their response URL is a
  non-opaque URL. Worker requests retain the existing 4 MiB IPC message limit;
  one worker task can queue at most 64 requests and 8 MiB of request payloads.
  Required replies use a separate broker lane capped at eight messages and
  16 MiB of queued payloads; if that lane overflows,
  the renderer session fails explicitly and uncommitted browser sessions abort
  rather than stalling the browser-wide database service.

This is not the complete IndexedDB API. Indexes, multiEntry/unique index
constraints, cursor direction over indexes, and worker realms have baseline
coverage, but full connection blocking/versionchange coordination, the newer
getAllRecords API, and complete cross-tab transaction scheduling are not
implemented. Worker termination sends an ordered browser-side retirement
control: earlier queued Steps finish first, then uncommitted sessions are
aborted and their capacity is released. A late Step from that Worker client
cannot recreate a session. Navigation, tab close, and renderer replacement
also retire affected sessions; the idle limit remains a fallback for abandoned
sessions. Stored data is subject to an alpha quota and is not a substitute
for a mature browser's durability guarantees.

## Dedicated Worker command mailbox

The document renderer delivers page messages, MessagePort messages and closes,
Fetch events, IndexedDB replies, and WebSocket events to each dedicated Worker
through one nonblocking mailbox. The mailbox holds at most 1,024 queued commands
per Worker and charges retained payloads against 32 MiB per Worker and 128 MiB
across Workers in one document. An in-flight command keeps its byte charge
until its Worker callback completes. These are implementation safety limits,
not limits defined by the HTML Standard.

If a command cannot be admitted, the renderer reports a Worker error and
retires that Worker; it does not block the document or silently discard the
message. Retirement sets an independent cancellation flag so a full mailbox
cannot prevent termination, aborts active Worker Fetch requests, retires its
browser-owned IndexedDB client, and cancels its live WebSockets. Late replies
cannot be delivered to the page or a replacement Worker. Unit tests cover
count and byte saturation, release after reception and disconnection, and
Fetch/IndexedDB/WebSocket teardown on overflow.

## Temporal and color input states

- Date, month, week, time, datetime-local, and color controls keep a live
  value separate from the default value. Reset restores the sanitized
  default, and invalid color values normalize to opaque black.
- valueAsDate uses UTC for date/month/week/time. datetime-local deliberately
  rejects valueAsDate because its wall-clock value has no time zone.
- valueAsNumber uses epoch milliseconds for date/week/datetime-local,
  milliseconds since midnight for time, and months since January 1970 for
  month. Invalid values yield NaN; non-finite setter values follow the
  applicable type/error behavior.
- Unit tests cover leap days, ISO week-year boundaries, UTC round trips,
  reset, sanitization, and error ordering.

## Reproduction and interpretation

The browser tests use the hidden benchmark/capture launcher and local
loopback fixtures. They do not display a Breeze or Chromium window. The
HTML5test measurement uses a fresh profile, a fixed 1.25 device scale, and
a settle period; compare the score only with runs under the same conditions.
The score is a capability inventory. It neither proves visual parity with
Chrome nor certifies WebSocket, IndexedDB, or form-control conformance.
