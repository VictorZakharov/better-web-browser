# ADR 0004: Browser-owned state and streaming Fetch broker

- Status: Accepted and implemented
- Date: 2026-08-23
- Issues: [#42](https://github.com/VictorZakharov/better-web-browser/issues/42),
  [#44](https://github.com/VictorZakharov/better-web-browser/issues/44)

## Context

The renderer is deliberately capability-free. It cannot own WinHTTP, ambient credentials, durable
cookies, or persistent Web Storage without defeating the process boundary in
[ADR 0001](0001-renderer-process-boundary.md). At the same time, synchronous APIs such as
`document.cookie`, `localStorage`, and `sessionStorage` need document-local state, and Fetch bodies
must cross the process boundary without an unbounded browser allocation.

The relevant compatibility authorities are:

- [RFC 10025](https://www.rfc-editor.org/rfc/rfc10025.html) for cookie storage and retrieval;
- the [Fetch Standard](https://fetch.spec.whatwg.org/) for credentials, forbidden headers,
  redirects, CORS, filtering, and cookies during Fetch;
- the [HTML Web Storage specification](https://html.spec.whatwg.org/multipage/webstorage.html) for
  local/session storage ownership and API behavior; and
- Microsoft's [`WinHttpQueryDataAvailable`
  documentation](https://learn.microsoft.com/windows/win32/api/winhttp/nf-winhttp-winhttpquerydataavailable)
  for incremental synchronous response reads.

## Decision

### Authority and projections

`BrowserApplication` owns one persistent `HttpClient` cookie authority and one persistent
`LocalStorage` authority. Each top-level `BrowserTab` owns its own `SessionStorage`. A renderer gets
only versioned, origin-filtered document snapshots:

- a non-`HttpOnly` cookie string;
- one local-storage area; and
- one session-storage area.

Renderer writes are typed intents carrying `DocumentId`; they are never durable writes. The browser
checks the active document, derives the URL/origin from browser state, applies the mutation to the
authority, and sends the resulting authoritative snapshot back. The renderer may update its local
projection optimistically for synchronous JavaScript semantics, but a rejected or stale write is
corrected by that snapshot. Old-document messages are discarded without regaining authority.

Each renderer `Hello`/`Ready` handshake is also bound to a typed, browser-issued
`BrowsingContextId`. The ID never appears in the command line and cannot be replaced by renderer
content.

Cookie snapshots preserve RFC retrieval order and duplicate names. `HttpOnly` cookies remain
available to HTTP Fetch but never enter a script-visible snapshot.

### Persistence

The default per-user profile is `%LOCALAPPDATA%\Breeze`, consistent with Microsoft's
[`FOLDERID_LocalAppData`
definition](https://learn.microsoft.com/windows/win32/shell/knownfolderid). Tests and isolated
automation may set `BREEZE_PROFILE_DIRECTORY` to an absolute directory.

`cookies.json` contains persistent cookies only; session cookies are removed at browser shutdown.
`local-storage.json` contains origin-keyed local storage; session storage is never serialized. Both
formats have explicit versions, hostile-input count/byte ceilings, strict validation on load, and a
temporary-file plus backup rename sequence so an interrupted write can recover. Invalid primary
state may recover from the last backup and otherwise fails closed rather than being interpreted
partially.

### `cookie_store` evaluation

The evaluated maintained release was
[`cookie_store` 0.22.1](https://crates.io/crates/cookie_store/0.22.1). Its crate and default direct
dependency manifests declare permissive MIT/Apache-2.0 licensing, so licensing was not a blocker.
Its default graph would add `cookie`, `publicsuffix`, `time`, `url`, `idna`, logging, and serde
support; Breeze already owns URL parsing, serde persistence, and public-suffix lookup through
existing dependencies.

The decision is **no-go for this integration**:

- the crate describes and implements an RFC 6265 store, while Breeze's accepted behavior targets
  RFC 10025;
- its URL-only retrieval interface cannot decide Fetch's request-context-dependent SameSite rules;
- Breeze would still need parallel enforcement for non-HTTP/`HttpOnly` access, cookie prefixes,
  insecure secure-cookie overlay protection, the 400-day lifetime cap, credentials, quotas, and
  versioned renderer snapshots; and
- replacing the tested in-tree store would therefore add an adapter and dependency graph without
  deleting the security-sensitive policy being reviewed here.

This is not a permanent rejection. Re-evaluate a future release if it exposes RFC 10025 storage and
context-aware retrieval hooks that let Breeze remove, rather than duplicate, policy code. No
dependency was added, so `THIRD_PARTY_NOTICES.md` does not change.

### Typed Fetch streaming

The renderer sends a bounded batch of Fetch intent heads and request-body chunks. The browser:

1. verifies the active `DocumentId`;
2. reconstructs `FetchRequest` from the browser's authoritative document URL;
3. rejects renderer control of cookie, origin, referrer, authentication, cancellation, and other
   guarded fields;
4. applies redirects, credentials, CORS/preflight, cookies, response filtering, and body limits;
5. reads the final WinHTTP response incrementally; and
6. sends a typed response head followed by ordered chunks and an end or abort message.

The network-worker-to-broker queue is synchronous and bounded. A full queue backpressures the
network worker. Browser command, renderer input, and browser UI-event queues are also bounded.
Presentation events coalesce to the newest immutable snapshot. Valid renderer Fetch batches have a
one-slot browser-event lane; its producer waits on the broker thread until the Win32 thread drains
the slot, preserving Fetch ordering while bounded pipe queues propagate backpressure to the
renderer. Queue producers explicitly wake the broker after enqueueing work; a 10 ms timed park
remains only as an idle fail-safe, not as the normal delivery cadence.

Other UI-event exhaustion fails the renderer session rather than growing browser memory or
blocking the Win32 message pump. Terminal exit data is retained outside that queue so overflow
cannot hide the crashed-tab surface. Fetch batches have both per-body and aggregate body limits.
The broker validates request IDs, offsets, lengths, ordering, and active-document identity in both
directions.

Navigation currently completes its bounded network body before transferring it through typed
length-declared chunks. Subresource responses are progressive from WinHTTP through broker IPC.

## Initial implemented limits

| Resource | Ceiling | Failure behavior |
| --- | ---: | --- |
| IPC control payload | 256 KiB | Reject frame before unbounded allocation |
| IPC frame payload | 8 MiB | Close renderer session |
| Fetch response chunk | 64 KiB | Reject stream contract |
| Queued Fetch chunks | 8 | Backpressure network worker |
| Queued browser commands | 8 | Reject without blocking the UI |
| Queued decoded renderer frames | 8 | Stop pipe reads until broker drains |
| Deferred browser messages in renderer Fetch wait | 64 | Reject the renderer session |
| Queued renderer UI events | 256 total; 2 presentations; 1 Fetch batch | Terminate renderer on overflow |
| Fetch requests per batch | 256 | Reject batch |
| Fetch request/response body | 16 MiB each | Typed body-too-large failure |
| Aggregate bodies per Fetch batch | 32 MiB | Reject batch/stream |
| Aggregate metadata per Fetch batch | 4 MiB | Reject batch |
| Cookie snapshot / assignment | 64 KiB / 4 KiB | Reject state message |
| Persistent cookies | 3,000 total; 180 per domain | Deterministic oldest-cookie eviction |
| Web Storage | 5 MiB and 1,024 entries per origin | `QuotaExceededError` projection |
| Persisted cookie/local-storage files | 16 MiB / 64 MiB | Reject persisted state |

The source of truth for these values is `src/limits.rs`.

## Consequences and remaining boundaries

- Persistent authority no longer exists in the renderer, and direct renderer networking remains
  denied by AppContainer launch policy and integration tests.
- Browser-side networking and IPC delivery are progressive and backpressured. The JavaScript
  `Response` body is still assembled by the renderer before its current stream object consumes it.
- Synchronous WinHTTP cancellation is cooperative around platform calls and between chunks; a call
  already inside WinHTTP may run until its configured timeout.
- Cross-document `storage` event broadcast, storage property-name traps, dynamic imports/import
  maps, Shared/Service Workers, and the wider browser API surface remain separate compatibility
  work.
- These stores are early browser implementations, not a security audit or a claim of complete
  RFC/HTML conformance.

## Verification contract

Regression coverage includes:

- expiry, domain/path, public suffix, Secure, HttpOnly, SameSite, prefix, deletion, ordering,
  eviction, persistent/session restart, and corrupt persisted-cookie cases;
- origin isolation, quotas, optimistic version rejection, persistence, recovery, and non-persistent
  session storage;
- round trips and malformed-size rejection for every new state and Fetch stream message;
- bounded producer backpressure, ordered stream assembly, progressive WinHTTP delivery, and
  cancellation between chunks;
- AppContainer state snapshot/mutation/correction behavior in one retained JavaScript realm; and
- direct renderer network denial, malformed IPC, crash, hang, and tab-local recovery.
