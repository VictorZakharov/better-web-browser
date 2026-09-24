# WebSocket, IndexedDB, and temporal form-control slice

This slice implements browser behavior behind feature probes; it does not
special-case HTML5test. The normative references are the
[WebSockets Standard](https://websockets.spec.whatwg.org/),
[Indexed Database API](https://w3c.github.io/IndexedDB/), and
[HTML input-state algorithms](https://html.spec.whatwg.org/multipage/input.html).
The implementation remains a bounded technical alpha, not complete
conformance to those specifications.

## WebSocket ownership

- The document realm has the WebSocket constructor, URL/protocol validation,
  open/message/error/close events, text and binary sends, binaryType, buffered
  amount, and close-code handling. The page cannot open a socket directly.
- The renderer sends a bounded command through its document broker. The
  browser resolves the real client and origin, applies the page's CSP
  connect-src and mixed-content policy, adds relevant cookies, and owns the
  WinHTTP connection and frame transport.
- Replies are document-scoped. Late frames for a retired navigation cannot
  reach a new page. Queues and payloads have explicit ceilings rather than
  unbounded renderer-to-browser memory use.
- The hidden integration fixture checks a real loopback handshake, Origin and
  subprotocol headers, a text frame, a binary frame, and a clean close while
  asserting that the retained document repaints.

This is a baseline for document WebSockets. Worker exposure, extension
negotiation, compression, and broader network interoperability remain work
to do; absence of these features must not be hidden behind a site-specific
fallback.

## IndexedDB ownership and behavior

- Profile persistence belongs to the browser process. The renderer can only
  request operations for a browser-resolved document/frame client. The page
  cannot choose another origin or an arbitrary profile path.
- Databases are partitioned by tuple origin. Opaque origins fail rather than
  sharing a global namespace. The browser bounds both individual IPC messages
  and on-disk origin/database size.
- The exposed window API covers asynchronous open, upgrade, delete, database
  listing, object-store creation/deletion, read-only and readwrite
  transactions, structured-cloned values, generated keys, key paths, get,
  getKey, put, add, delete, clear, count, getAll, getAllKeys, key ranges, and
  forward/reverse object-store cursors.
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

This is not the complete IndexedDB API. Indexes, multiEntry/unique index
constraints, worker realms, full connection blocking/versionchange
coordination, cursor direction over indexes, the newer getAllRecords API,
and complete cross-tab transaction scheduling are not implemented. Stored
data is subject to an alpha quota and is not a substitute for a mature
browser's durability guarantees.

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
