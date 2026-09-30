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
AAC-LC, Ogg/Vorbis, native FLAC, audio-only WebM/Vorbis, and Ogg/FLAC.
FLAC uses a contained pure-Rust decoder
without requiring a host Media Foundation FLAC codec.
The media worker decodes audio to PCM, drives the XAudio2 output path,
and supports play, pause, volume, and seek without fabricating a video frame.
`HTMLMediaElement.canPlayType()` makes the conservative `maybe` claim for
`audio/flac` and its deprecated `audio/x-flac` alias.
`audio/ogg; codecs="vorbis"`, `audio/ogg; codecs="flac"`, and
`audio/webm; codecs="vorbis"` report `probably`, while codec-less Ogg and
audio WebM report `maybe`; Opus and WebM video remain unsupported.
Source selection and script queries share the same MIME parsing/support matrix,
including quoted parameter handling and first-valid duplicate parameters.
Media Capabilities distinguishes complete-file FLAC from unsupported Media Source
FLAC. `decodingInfo()` validates one-track MIME and Web IDL dictionaries, then
applies conservative channel, sample-rate and video-output limits; it is not a
per-host decoder or performance measurement. `smooth` and `powerEfficient`
remain false. See the [configuration limits](encoded-audio-containers.md).
`audio/m4a` and `audio/x-m4a` share the proven
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

The [complete-file container limits](encoded-audio-containers.md) are important:
other Ogg codecs, Opus, WebM video/multiple tracks, DRM, and
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

`rel=prefetch` admits up to two distinct hints per top-level document, for
HTTP(S) documents and subresources on the same or another origin. Its Fetch
destination is empty even when `as` names a resource type. Absent `crossorigin`
uses no-CORS and included credentials; `anonymous` and `use-credentials` use
CORS with same-origin and included credentials respectively. The link's
`referrerpolicy` applies, while `type` and `media` do not change the prefetch
destination or gate this hint. The browser derives the owner URL and policy,
checks CSP's resource-hint source-list union on every redirect, consumes the
body under a 2 MiB cap, and never exposes speculative bytes to the renderer.
Completed HTTP responses (including 404) fire `load`; network/CORS failures
fire `error`. Prefetch does not delay the document's `load`.

Only complete responses with explicit freshness or validators enter the bounded
private cache; immediate reuse requires freshness. Same-origin future navigation and compatible subresource loads
can reuse them when URL, owner origin, effective credentials, Cookie, referrer,
Origin, and `Vary` agree. A cross-origin no-CORS prefetch cannot become a
readable CORS response through cache reuse. Cookie changes and nonmatching
request modes cause a normal network fetch. The browser may decline work
beyond the quota or size cap; embedded-document hints and `integrity`-bearing
prefetch links remain a conservative unsupported subset. This functional
top-level processing model is now advertised by `relList.supports('prefetch')`.

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
