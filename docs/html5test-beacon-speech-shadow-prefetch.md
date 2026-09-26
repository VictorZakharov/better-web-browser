# Web-platform batch: Beacon, speech, slots, prefetch, media, and CSP

This batch implements browser behavior behind several HTML5test capability rows.
The score is a secondary inventory: neither a returned `canPlayType()` value nor the
presence of a JavaScript method is treated as sufficient proof of support. The
acceptance tests exercise real requests, composed-tree layout, native decoding,
document retirement, and policy enforcement.

## Beacon delivery

`navigator.sendBeacon()` synchronously serializes an HTTP(S) POST and queues it
through the document's Fetch client. The browser reconstructs the request from a
bounded renderer intent, reapplies the document origin and CSP, and owns delivery
after the page navigates away. The response is never exposed to page script.
Cross-origin requests follow the Fetch no-CORS/CORS distinction appropriate to
their content type. An untrusted renderer cannot choose a different method,
credentials mode, destination, or arbitrary headers.

Admission bounds one unsent renderer action batch to 64 KiB and 64 requests; a
separate browser-wide limit permits at most 32 delivery workers at once. The
batch budget is released when actions leave the JavaScript realm, not when a
network request completes. Thus this is a bounded best-effort Beacon subset,
not a complete implementation of the shared outstanding keepalive quota.
`fetch()` with `keepalive: true` is not implemented by this change. A `true`
return from `sendBeacon()` means the action was queued, not delivered.

References: [Beacon](https://www.w3.org/TR/beacon/) and
[Fetch keepalive requests](https://fetch.spec.whatwg.org/#request-keepalive).

## Manual slot assignment

`attachShadow({ slotAssignment: "manual" })` creates a manual-assignment root.
`ShadowRoot.slotAssignment` reports its mode, and `HTMLSlotElement.assign()`
stores an ordered set of element/text slottables. Distribution is limited to
direct children of the slot's host; stale weak references do not retain removed
nodes. `assignedSlot`, `assignedNodes({ flatten: true })`, composed-tree
traversal, layout/style invalidation, and `slotchange` use the same assignment
state. Named-slot roots continue to use named assignment.

Reference: [HTML slot assignment](https://html.spec.whatwg.org/multipage/scripting.html#dom-slot-assign).

## Document prefetch

`<link rel="prefetch">` can speculatively fetch a same-origin, top-level
document into the browser's existing private HTTP cache. The browser, rather
than the renderer, constructs the future-navigation cache identity and checks
the document's CSP resource-hint rule. Only complete cacheable responses are
reused; redirects are not speculatively followed, cookie changes partition
reuse, duplicate URLs are not fetched again, and each document may admit at
most two distinct URLs. Prefetch failures do not fail the document.

This is intentionally narrower than the full HTML link-processing model:
cross-origin and non-document prefetches are not supported, and
`relList.supports("prefetch")` remains an underclaim until that model is
complete. The hidden browser test inserts a link, observes its load event,
navigates, and verifies one network GET serves both stages.

References: [HTML prefetch](https://html.spec.whatwg.org/multipage/links.html#link-type-prefetch)
and [CSP resource hints](https://www.w3.org/TR/CSP3/#does-resource-hint-request-violate-policy).

## Media types and native decoding

`audio/m4a` and `audio/x-m4a` select the existing AAC-in-ISO-BMFF decoder;
video-only codec declarations are not accepted under audio media types. A
contained-renderer test supplies real fragmented AAC media under an M4A
response type and verifies the audio decoder is reached.

Native `audio/flac` and its legacy `audio/x-flac` alias use the Windows Media
Foundation FLAC decoder when present. A self-authored one-second fixture is
decoded, played on a silent clock, paused, and sought inside the contained
media worker. `canPlayType()` returns the conservative `maybe`; Ogg FLAC and
FLAC in Media Source Extensions are not claimed. `MediaCapabilities`
configuration checks currently derive support from media-type policy, not a
per-host decoder query. The contained-worker capability probe allows tests to
skip only the host-dependent waveform path when the native decoder is absent.

Reference: [FLAC media type registration (RFC 9639)](https://www.rfc-editor.org/rfc/rfc9639.html).

## Script-generation policy

An enforcing response or parser-inserted CSP disables V8 string code
generation before author script runs. A dynamically inserted meta policy
takes effect before the inserting DOM call returns. Dedicated workers also
inherit the applicable eval/Wasm generation policy after their trusted
bootstrap. This is a policy-enforcement slice, not a claim of complete CSP3
coverage; tests cover the immediate document and worker boundaries.

Reference: [Content Security Policy Level 3](https://www.w3.org/TR/CSP3/).

## Speech synthesis

The JavaScript API and its Windows SAPI worker are described in
[Web Speech synthesis](web-speech-synthesis.md). Native audio is browser-owned,
bounded, and cancelled on document retirement. `speak()` requires trusted
transient activation as a documented Breeze policy; speech recognition and
word/mark boundary events remain unsupported.
