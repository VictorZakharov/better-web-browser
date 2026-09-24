# Resource loading, font loading, responsive images, and forms

This batch extends web-platform behavior across the fetch pipeline, document loader,
script realm, and layout engine. It is not keyed to HTML5test URLs or probes. The
contracts below are based on [Fetch](https://fetch.spec.whatwg.org/),
[Subresource Integrity](https://www.w3.org/TR/SRI/),
[HTML link types](https://html.spec.whatwg.org/multipage/links.html),
[responsive images](https://html.spec.whatwg.org/multipage/images.html),
[CSS Font Loading](https://www.w3.org/TR/css-font-loading-3/),
[WOFF2](https://www.w3.org/TR/WOFF2/), and
[HTTP caching](https://www.rfc-editor.org/rfc/rfc9111.html).

## Fetch and response admission

- The private browser-process HTTP cache admits a completed 200 GET response only
  after its final byte has arrived. An aborted or partial stream cannot satisfy a
  later fetch. Requests carrying authorization, a range, explicit validators, or
  `no-store` bypass cache admission. `Set-Cookie`, `Content-Range`, and responses
  without explicit cache freshness are not admitted.
- Entries are partitioned by URL, origin, request context and destination, mode,
  credentials, and outbound cookie state. `Vary` additionally compares the named
  request fields. A changed `Vary` policy invalidates incompatible older
  representations. This is deliberately stricter than a shared HTTP cache.
- Explicit `Cache-Control: max-age`, `Expires`, `Date`, and `Age` determine
  freshness. The cache does not guess heuristic freshness. A stale response with
  `ETag` or `Last-Modified` is validated; a successful 304 updates selected
  headers while retaining the stored body. A request with `no-cache`,
  `max-age=0`, or `Pragma: no-cache` validates even a fresh entry.
- Fetch's `default`, `reload`, `no-store`, `no-cache`, `force-cache`, and
  `only-if-cached` modes have distinct cache paths. `only-if-cached` cannot
  silently issue a network request on a miss. Cached responses still pass the
  same response-type and CORS checks as new responses.
- Memory limits are 64 entries, 2 MiB per entry, and 24 MiB in total. This is
  a memory cache, not a persistent disk cache or a service-worker cache.

## Subresource Integrity and preloads

- External scripts, stylesheets, module scripts, and matching preloads read
  element-owned `integrity` metadata. Fetch `Request.integrity` validates the
  complete byte stream before exposing a response to script. Verification uses
  the strongest supported SHA-256/384/512 algorithm present; any matching hash
  at that strength succeeds. Unknown or malformed entries do not become an
  implicit denial. Opaque responses are ineligible for a usable hash.
- Validation is against fetched bytes, before text decoding or CSS/script
  interpretation. A mismatched script does not execute; a mismatched
  stylesheet does not enter the cascade. A failed preload does not authorize
  its later consumer. Diagnostics identify a digest mismatch or ineligible
  response without printing resource bytes.
- `rel=preload` supports script, style, image, and font destinations; the
  modulepreload path uses module fetch settings and checks JavaScript MIME.
  Preload matching includes URL, destination, mode, credentials, referrer
  policy, integrity metadata, module kind, and nonce. A preload does not apply
  CSS or execute JavaScript by itself. Matching consumers can reuse validated
  bytes; mismatched settings take an ordinary fetch path.
- The document-scoped preload response store is bounded to 32 responses and
  8 MiB. A failed response, invalid MIME, hash mismatch, or budget rejection
  must not be mistaken for a successfully loaded resource. A later
  authoritative stylesheet or module may retry.
- Image preload hints honor `imagesrcset` and `imagesizes`; an invalid source
  set falls back to `href`. Hints with unsupported `as` or `type` are ignored.
  A hint is an optimization, never authority to execute or apply content.

## Fonts and images

- `FontFace`, `FontFaceSet`, `document.fonts`, `load`, `ready`, membership,
  status, and loading events are tied to actual font fetch/decode/install
  outcomes. URL-backed faces remain pending until their fetch completes;
  ArrayBuffer-backed faces decode before they report loaded. CSS-connected
  `@font-face` rules appear in the set but are not deletable through it.
- WOFF2 is decoded with the existing Rust font path after preflight size/table
  checks and post-decode size checks. A declared WOFF2 source is preferred
  over a WOFF fallback when supported; failed or oversized input does not
  become an installed font. The decoder dependency is MIT-licensed and is
  recorded in the locked dependency and third-party notices workflow.
- `<img>`, `<picture>`, and image preloads share source-set selection.
  Density descriptors use the actual device pixel ratio. Width descriptors
  use the first matching `sizes` condition and its resolved source size;
  commas inside CSS functions are not candidate separators. Invalid
  candidates are skipped, and `src` participates as a 1× fallback only where
  appropriate. Picture source media and supported image MIME types decide
  which source set participates.
- `<input type=image>` is a replaced image control, not a text box. It
  requests its image resource, exposes width/height and alt reflection, and
  submits `.x` and `.y` entries derived from the click location. Direct
  `FormData(form, imageSubmitter)` has the specified zero-coordinate default.
- Programmatically assigned `input.files` accepts `FileList` objects, mirrors
  bounded basenames to the native control for the fakepath value and required
  validation, and includes the selected `File` objects in FormData. Clearing,
  reset, and type changes release the selected files. A nonempty assignment
  to `input.value` is rejected. No native file picker is claimed in this batch.

## Regression and safety boundaries

The test suite exercises raw-byte hash matching, strongest-hash selection,
opaque rejection, parser script order, stylesheet cascade admission, preload
reuse and failed-preload retry, module MIME failure, cache freshness,
conditional validation, Vary, authorization isolation, incomplete streams,
entry-size limits, responsive image DPR and sizes, WOFF/WOFF2 safeguards,
FontFace load and `ready`, image-button form coordinates, and FileList/FormData
identity. Renderer-process tests must run as the normal Windows user with
their hidden-process launch paths; browser screenshots and benchmarks use
`scripts/run-hidden-benchmark.ps1` only.

The 2026-09-24 fresh-profile hidden release run at 1280×720, 125% scale,
`en-US`, and 10 seconds' settle displayed **401 / 588** on HTML5test.co,
up from the preceding **396 / 588** release observation. The response was
HTTP 200 with zero JavaScript errors, no renderer exit, and 123 rows marked
missing by the diagnostic selector. The screenshot and JSON diagnostics are
local benchmark artifacts, not committed test fixtures. The score does not
measure the cache's freshness rules, SRI failure handling, font decoder
safety, file-control behavior, or pixel fidelity.

This is a bounded baseline, not a claim of complete specification coverage.
The cache has no disk persistence, heuristic freshness, or shared-cache
semantics. Preloading does not yet implement a full recursive module graph or
all HTML destinations and priorities. Font coverage does not imply complete
CSS font feature support or complex-script parity. Responsive images may make
a different permissible candidate choice than Chrome under unstable viewport
or network conditions. The file control does not open the operating-system
file picker. SRI owners that share a deduplicated URL but have conflicting
hashes currently fail closed as a group; independently admitting each owner
requires separating their resource lifecycle in a later slice.
