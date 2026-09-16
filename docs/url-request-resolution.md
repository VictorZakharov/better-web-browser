# URL parsing and native request resolution

This slice separates the [URL API parser](https://url.spec.whatwg.org/#concept-url-api)
from the browser's internal request algorithms. It addresses an observed modern
DuckDuckGo failure in which a page-installed URL implementation threw `Invalid
scheme` inside `XMLHttpRequest.open`. There are no hostname checks or site shims.

## Contract

- Public `URL`, `URL.parse`, and `URL.canParse` require an explicit base for relative
  input. A provided invalid base fails even for absolute input. `href` assignment
  parses without a base. Required arguments, scalar-value string conversion,
  conversion exceptions, and receiver branding are checked before parse results
  become exceptions, null, or false.
- [Request](https://fetch.spec.whatwg.org/#dom-request),
  [Response.redirect](https://fetch.spec.whatwg.org/#dom-response-redirect),
  [XHR.open](https://xhr.spec.whatwg.org/#the-open()-method), and Worker construction
  use native parsing and the realm's API base, not the author-visible `URL`
  constructor, prototype getters, or a mutable JavaScript location object.
  Documents use the first connected `base[href]` or their fallback URL; workers use
  their source URL. Invalid XHR URLs throw `SyntaxError` before canceling an
  existing request. Input-conversion exceptions retain their original identity.
- Base lookup is cached against document identity, connected mutation revision,
  and fallback URL. Insertion, removal, reordering, attribute changes, and history
  URL changes cannot reuse a stale cache entry. No-base documents are not scanned
  once per request while their tree remains unchanged.
- Window and worker realms share one URL/URLSearchParams implementation and native
  dispatch. Private state keeps URL and query-list updates connected. Iteration is
  live across deletion, sorting, and replacement through `URL.search` or `href`.
  Query parsing tolerates malformed percent escapes and replaces malformed UTF-8;
  serialization uses the form-urlencoded encode set. Request/Response form bodies
  use the same native byte parser, preserving BOMs and literal leading question
  marks rather than applying the URLSearchParams constructor's prefix stripping.
- Web setters use the existing `url` dependency's browser-oriented `quirks` API,
  rather than ad-hoc host/port parsing. The opaque-path state also preserves the
  encoded space immediately before a query or fragment when those are removed.
  Navigation scheme/credential policy and the existing URL-size budget remain.

## Verification and remaining limits

The owned tests exercise explicit versus implicit bases, conversion errors,
constructor replacement, failed-open atomicity, live base changes and history,
worker requests, private form serialization, and malformed form bytes.
`benchmarks/alpha/fixtures/url-resolution.html` makes actual loopback XHR/Fetch
requests under a conforming replacement and a throwing public constructor. It is
part of the hidden Breeze/Chrome alpha matrix, not just an API-existence check.

The curated gate adds static URL parsing, setter control-character handling, the
form-urlencoded parser, and the remaining query-list method files. Upstream files
are unchanged, with no expected failures. The broader constructor/origin/setter
corpus still exposes parser-library differences, including file paths, IDNA hosts,
opaque origins and uncommon scheme paths. These are **not** silently counted as
passes or a claim of complete URL conformance. The independent discovery manifest
keeps them runnable with passing expectations, so a failing command remains red:

```powershell
./scripts/checkout-wpt.ps1 -Destination G:/Git/wpt-url-discovery -Manifest tests/wpt/url-parser-discovery.json
./scripts/run-wpt.ps1 -WptRoot G:/Git/wpt-url-discovery -Manifest tests/wpt/url-parser-discovery.json -SkipBuild -Jobs 4
```

The final discovery run records 1,500 passing and 85 failing assertions across
those three files, with zero timeout/crash outcomes. The green curated URL cluster
is 16 files / 472 assertions; the full curated suite is 380 files / 3,354 assertions.

This does not add synchronous XHR, cross-origin workers, a complete Fetch/Worker
implementation, or complete HTML base-element freezing across every lifecycle.
Those policies and gaps are separate from fixing internal dependence on an author
constructor. No dependency or third-party source was added. Live search usability
and load performance require separate measurement; successful URL tests alone do
not establish either.

## Diagnostic-report containment

The first release comparison after correcting URL resolution progressed further
on modern DuckDuckGo, then stopped the renderer in all three runs: its console
output exceeded the strict IPC report count. A field-specific diagnostic confirmed
`runtime console count`; this was not a network failure or a successful page load.

Diagnostic lanes (errors, console, and diagnostics) are now bounded before
serialization, retaining their prefix and appending an explicit omission/truncation
notice. Each lane stays within the existing 512-entry limit and a stricter 64 KiB
aggregate text budget, with UTF-8-safe truncation. The wire decoder remains strict;
cookies, history, navigation, and other operational updates are not dropped through
this path. This bounds transported diagnostics, not all script-side allocation.
Unit tests round-trip oversized reports through the real codec, and an isolated
renderer test verifies that noisy author output cannot prevent the next page task.
