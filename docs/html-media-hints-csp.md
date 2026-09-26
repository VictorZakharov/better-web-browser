# HTML media, origin hints, and CSP policy delivery

This batch closes related gaps in how an HTML document selects resources, starts
network work, and reports the result to script. Capability probes are only a
starting inventory: the acceptance evidence includes actual contained-media
decode/play/seek and browser-authoritative policy tests.

## URL-backed audio

An HTML `<audio>` element with a `src` URL, or a usable child `<source>`, is
fetched with the `audio` Fetch destination and installed in the contained media
worker. Source selection checks declared MIME type and codecs before choosing
an element. A failed child `<source>` receives its error event and selection
continues to the next candidate; a response from a superseded `src` cannot
replace the current selection. The selected final response URL is exposed
through `currentSrc`. The renderer distinguishes network failure from decode
failure and dispatches the appropriate media error; an early `play()` request
waits for loading to finish instead of being rejected simply because no
decoder is ready yet.

The tested complete-resource formats are PCM WAV, MP3, AAC in M4A, ADTS
AAC, and native FLAC on Windows hosts with a Media Foundation FLAC decoder.
The media worker decodes audio to PCM, drives the XAudio2 output path,
and supports play, pause, volume, and seek without fabricating a video frame.
`HTMLMediaElement.canPlayType()` makes the conservative `maybe` claim for
`audio/flac` and its deprecated `audio/x-flac` alias, without claiming Ogg
FLAC. Media Capabilities distinguishes complete-file FLAC from unsupported
Media Source FLAC, but derives `decodingInfo()` support from MIME/type policy,
not a per-host native decoder query; a host without that optional decoder may
still report file support. `audio/m4a` and `audio/x-m4a` share the proven
AAC-in-MP4 decoder path with `audio/mp4`; a codec hint for a video-only M4A
still returns unsupported. A complete H.264-only fragmented MP4 uses a
worker-owned monotonic playback clock rather than requiring a fabricated audio
track; contained tests cover play, pause, seek, end, and replay. Media Source
can also accept separate H.264 video and AAC audio SourceBuffers.

Media `crossorigin` follows HTML's potential-CORS modes: `anonymous` and
`use-credentials` requests use browser-validated CORS before bytes reach the
renderer. The absent-attribute no-CORS path is intentionally narrower than
HTML's credentialed default for cross-origin media: it keeps credentials
same-origin because the current decoder path receives opaque encoded bytes in
the renderer. Sending cross-origin cookies with those bytes would make an
untrusted renderer able to read cookie-protected cross-origin responses.

Remaining media boundaries are important: Ogg/WebM audio, DRM, and
detached `new Audio(src)` resource discovery are not implemented here. The
latter needs script-originated resource discovery and detached-node lifetime
through the renderer host, not merely an `Audio` constructor property.

## Origin hints

Connected `link` elements with `rel=dns-prefetch` or `rel=preconnect` can
schedule an origin-only DNS warmup. HTML allows preconnect to stop after DNS
under resource constraints; this implementation does **not** claim to have
opened a reusable TCP or TLS connection. Hints make no extra HTTP request,
do not delay the document's `load` event, and are bounded by per-document and
browser-wide admission limits. They are lower priority than normal resources.
Only HTTP(S) origins without URL credentials are eligible.

The first `rel=prefetch` subset handles up to two same-origin HTML document
URLs per top-level document. The browser rechecks the live CSP and origin,
then fetches with the origin, referrer, credential mode, and private-cache
partition of a future navigation. It consumes the complete body before cache
admission, never exposes speculative bytes to the renderer, and rejects
redirects or responses over the 2 MiB per-entry budget. A completed link
receives `load` or `error`, but does not delay the current document's `load`.
Only cacheable responses with server-supplied freshness can be reused; an
ordinary navigation still goes to the network otherwise. Cookie changes
partition the cache and prevent reuse across credential states. Cross-origin
and non-document prefetches, `as`, `crossorigin`, `integrity`, `media`, and explicit
referrer policy are not yet supported. Consequently, `relList.supports()`
does not advertise full `prefetch` support.

## CSP policy delivery and timing

Response `Content-Security-Policy` headers are copied into the renderer's
document policy before any script or speculative resource starts. A valid
`<meta http-equiv="Content-Security-Policy">` in `head` adds an enforcing
policy when the parser reaches or script inserts the element; it does not
replace response policies or retroactively change earlier requests. The
browser receives ordered policy updates and rechecks subsequent privileged
fetches and origin hints against the current document policy. Meta-delivered
`report-uri`, `frame-ancestors`, and `sandbox` are ignored as specified.
Parser-delivered and dynamically inserted meta policies also update V8's
string-code-generation gate before subsequent author code can call `eval()` or
`Function()`; installing a policy during a script takes effect before the DOM
insertion call returns. Response CSP is synchronized into that gate before the
first author script. Focused tests exercise all three timings.

The implementation intentionally fails closed for *known security-sensitive*
directives it cannot enforce. Unknown directives and invalid individual
source expressions do not discard otherwise valid policy siblings. Inline
script elements match CSP3 SHA-256/384/512 hash sources against their exact
UTF-8 text (including base64url normalization), with every enforcing policy
checked independently. Style hashes, `unsafe-hashes` for attributes, report-only
policies, and full CSP3 coverage are not claimed. A `fetch()` called before a
dynamic meta insertion in the same JS task
can be conservatively blocked if its asynchronous serialization reaches the
browser after the new policy; preserving exact invocation-time ordering needs
a separate trusted request-sequencing design.

## Verification

The focused Rust and hidden-renderer tests exercise source fallback, PCM/MP3/
AAC decoding, media request destinations, selected URLs, early play promises,
404/corrupt-media errors, and contained play/seek. CSP tests cover header and
meta composition, parser and dynamic-insertion checkpoints, browser-side
request admission, hint gating/quotas, and IPC ordering. The current
HTML5test score is reported separately in the README with its dated hidden
release-run conditions; it is not a conformance claim.

Primary references:
[HTML media](https://html.spec.whatwg.org/multipage/media.html),
[HTML links and resource hints](https://html.spec.whatwg.org/multipage/links.html),
[Fetch request destinations](https://fetch.spec.whatwg.org/#concept-request-destination),
[CSP Level 3](https://www.w3.org/TR/CSP3/).
