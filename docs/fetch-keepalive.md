# Fetch keepalive request lifetime

The Fetch Standard gives a request with `keepalive: true` permission to outlive the global
that created it. The flag is not a different HTTP method or a fire-and-forget response mode:
while the document is alive, `fetch()` still resolves to an ordinary `Response` and can be
aborted by its signal. See [Fetch §2.2.5](https://fetch.spec.whatwg.org/#requests) and
[the `Request.keepalive` getter](https://fetch.spec.whatwg.org/#dom-request-keepalive).

Breeze carries the flag from `Request` through its script host and bounded renderer protocol.
The renderer flushes the upload before an immediate navigation mutation. The browser
validates a script-initiated intent and reconstructs its origin, client policy,
credentials, referrer, method, and headers before it admits the upload. An admitted request
keeps that browser-owned snapshot if navigation replaces the renderer document. It is not
combined with the old document's abort signal, but an explicit `AbortSignal` remains able to
cancel it while the initiating realm is active. If the realm survives until the response,
the normal streamed response path and Promise remain available; otherwise the request may
finish without exposing a response to the retired realm.

The Fetch Standard caps the aggregate in-flight keepalive body bytes in a fetch group at
64 KiB. Breeze uses a conservative per-document 64 KiB/32-request admission budget,
shared by `fetch(..., {keepalive: true})` and `navigator.sendBeacon()`. Streaming request bodies
are rejected by `Request`; the browser independently checks the wire body's size and quota.
An additional browser-process-wide 64-request cap bounds queued survivors across repeated
navigations and tabs.
This limit can reject more requests than a distinct nested fetch group would in a browser
that models each group separately. A tab or browser-process exit is not a durable background
delivery guarantee, and neither Fetch nor Beacon promises successful delivery.
Beacon's synchronous `true` result currently reflects renderer-side queue admission; a later
browser-wide cap or worker-start failure can still discard it without a callback. This is a
known accounting gap, not a claim that `true` guarantees network transmission. Such a
one-way failure never stops the page engine.

Acceptance evidence is split across script Request copy/limit tests, pointer-free protocol
round-trip and forged-initiator tests, contained renderer response-Promise tests, browser
quota tests, and hidden local-server tests that observe a POST after navigation and no POST
after an explicit pre-navigation abort.
These tests verify real HTTP delivery and browser ownership, not a feature-detection string.
