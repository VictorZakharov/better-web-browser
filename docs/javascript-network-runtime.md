# JavaScript networking, modules, and workers

Breeze exposes web networking through one retained JavaScript realm per document and one isolated
realm per dedicated worker. The implementation follows the [Fetch Standard](https://fetch.spec.whatwg.org/),
[XMLHttpRequest Standard](https://xhr.spec.whatwg.org/), [HTML module-script and worker
algorithms](https://html.spec.whatwg.org/multipage/webappapis.html), [Web IDL exception
bindings](https://webidl.spec.whatwg.org/#idl-DOMException), and the cookie processing model in
[RFC 10025](https://www.rfc-editor.org/rfc/rfc10025.html).

## API surface

The document and dedicated-worker globals share:

- `fetch`, `Request`, `Response`, and guarded iterable `Headers`;
- the Body mixin (`arrayBuffer`, `blob`, `bytes`, `formData`, `json`, and `text`) with clone,
  lock, disturbance, and one-consumer rules;
- `Blob`, `File`, `FormData`, `URLSearchParams`, `TextEncoder`, and `TextDecoder` body sources;
- `ReadableStream`, `WritableStream`, and `TransformStream` primitives used by request and response
  bodies;
- `AbortController`, `AbortSignal.abort`, `AbortSignal.timeout`, `AbortSignal.any`, abort reasons,
  and trusted abort events; and
- asynchronous `XMLHttpRequest`, including state transitions, response types, upload/download
  progress events, timeout, abort, response-header filtering, and Fetch-backed CORS/credentials.

Fetch and XHR emit typed actions from the owning realm. The renderer serializes bounded intent
fields; the browser reconstructs each request from its authoritative document URL, applies the
shared Fetch policy, and runs WinHTTP. Response heads and bounded chunks cross a backpressured IPC
stream before completion is routed to the originating context, document, realm, and request ID. A
completion for a navigated or closed document is discarded. Abort removes the pending JavaScript
operation immediately and cancels further browser-side work at the next safe transport boundary.

## Modules and dedicated workers

Classic scripts and ECMAScript modules use distinct fetch modes. Module graphs resolve relative and
absolute URL specifiers, cache each module by URL, enforce CORS and JavaScript MIME types, expose
`import.meta.url`, report graph failures on the owning script element, and support top-level await.
Document lifecycle completion and script `load`/`error` events wait for asynchronous module
evaluation to settle.

`Worker` creates an isolated V8 realm on a background thread. Classic and module dedicated workers
support structured-clone messaging and transfers, timers, Fetch/XHR, relative static imports,
`importScripts` for classic workers, top-level await for module workers, and deterministic
termination. Messages sent while a module worker is evaluating are queued until its top-level
promise fulfills; evaluation rejection closes the worker and reports an error to its owner.

## Cookies and Web Storage

The browser-owned cookie jar applies domain/path matching, host-only cookies, default paths,
`Expires`/`Max-Age` precedence and the 400-day cap, `Secure`, `HttpOnly`, `SameSite`, secure-overlay
protection, `__Secure-`/`__Host-` prefixes, public-suffix rejection through `psl2`, deterministic
ordering, and per-domain/global eviction limits. Script reads omit `HttpOnly`; script writes cannot
create it. Fetch credentials and schemeful-site context decide which stored cookies accompany a
request.

Persistent cookies and origin-scoped `localStorage` are owned and versioned by the browser process.
`sessionStorage` is owned by its top-level tab and is never serialized. Renderer realms receive only
document-scoped projections and submit typed mutation requests; the browser always replies with the
resulting authoritative snapshot. See
[ADR 0004](architecture/0004-browser-state-and-fetch-broker.md) for persistence, quotas, and the
`cookie_store` dependency evaluation.

## Deliberate current boundaries

This is a usable core, not the entire browser API surface:

- subresource responses are read incrementally by WinHTTP and cross renderer IPC with bounded
  backpressure, but the JavaScript realm currently receives the completed body rather than a
  progressively delivered `Response.body` stream;
- synchronous XHR on `Window` is intentionally rejected; `responseXML` remains `null` until the
  XML/HTML `DOMParser` path exists;
- static module graphs are supported, while network-discovered dynamic `import()`, import maps, and
  module types other than JavaScript remain future work; and
- dedicated workers are implemented; shared workers, service workers, worklets, and their storage,
  lifecycle, interception, and registration models are not.

Resource ceilings remain part of the contract: request/response bodies, aggregate page resources,
script bytes, dynamic script count, worker lifetimes, and execution budgets are bounded. See
[security-and-fuzzing.md](security-and-fuzzing.md) for the current limits.

## Verification

Unit tests cover Web-IDL conversions, body and stream state, Fetch/XHR events, cookie/storage policy,
module graphs, top-level await, structured clone, and worker lifecycle. Hidden loopback integration
tests exercise real Fetch/XHR completions, progressive response delivery, renderer state
snapshot/mutation/correction, external module dependencies, and a module worker whose top-level
fetch must settle before its queued message runs. The pinned curated WPT gate adds upstream Fetch,
XHR, Abort API, Web IDL, and module-lifecycle cases without expected-failure allowances.
