# Dynamic JavaScript modules

Document realms support native `import()` and dynamically connected inline/external
`script type="module"` elements. This is an HTTP(S) JavaScript-module slice, not a
claim of complete module or HTML loading conformance.

## Contract

- V8 creates the actual import promise and module namespace. Concurrent imports
  share the URL-keyed source/compiled-module maps and evaluate a module once.
  Private native promise resolvers are not exposed as page-global helper functions.
- Fetch completion prepares transitive graphs without evaluating author code.
  Completion wakes an otherwise idle document; no polling timer or synchronous
  network wait is needed. Timers/rendering continue while sources are pending.
- The promise fulfills only after evaluation, including top-level await, and
  rejects with the original thrown value. Parsed/evaluation errors remain cached;
  failed HTTP/MIME/network fetches notify current waiters and permit later retries.
- Import requests use CORS, JavaScript MIME validation, and the referring script's
  module credentials/referrer-policy options. The no-CORS transport credentials
  of ordinary classic scripts do not become cross-origin module credentials.
  Dynamic module bodies decode as UTF-8, ignoring legacy transport charset labels.
- Dynamic graph redirects retain the requested URL as module identity and use the
  final response URL for relative dependencies and `import.meta.url`. Inserted
  inline modules freeze the document base URL at preparation, while retaining
  distinct internal module identities for different elements.
- Inserted modules are asynchronous, including dependency-free inline modules.
  Explicit `async=false` shares the insertion-ordered list with dynamic external
  classics. A pending top-level-await promise does not hold that list or window
  load; pending element graph fetching does. Bare `import()` is not a window-load
  blocker. `document.currentScript` is null inside modules.
- External owners receive load/error events. Inline module success does not fire
  an external-file load event. Resource errors remain distinct from module parse,
  link, and evaluation errors. Navigation discards old document jobs and promises.
- Existing watchdog and source-byte limits remain active. Unsettled import jobs
  and dynamic fetch identities are bounded by `MAX_DYNAMIC_SCRIPTS` (32), and the
  retained module map by `MAX_PAGE_SCRIPTS` (64), including empty sources.

The primary contracts are HTML's
[script preparation/execution](https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element),
[HostLoadImportedModule](https://html.spec.whatwg.org/multipage/webappapis.html#hostloadimportedmodule),
and [single-module fetching](https://html.spec.whatwg.org/multipage/webappapis.html#fetch-a-single-module-script).
In particular, successful compiled records and failed fetches have different cache
lifetimes; treating every rejection as a permanently cached failure is incorrect.

## Verification

Runtime regressions cover namespace/evaluation identity, concurrent waiters, TLA,
timers, thrown values, parse-error identity, retry, cancellation, credentials,
frozen inline bases, distinct inline records, ordered mixed scripts, and map bounds.
Hidden renderer tests additionally cover real HTTP redirects and retry after 404,
transitive loading, strict MIME rejection, and network completion on an idle page.

The owned `benchmarks/alpha/fixtures/dynamic-modules.html` fixture is compared with
muted unified-headless Chromium. It tests inserted inline/external owners and
`import()` together, and exposes its individual assertions as `#main[data-facts]`.
Its completion title is `Dynamic modules PASS`. Trace display is sorted only after
completion: network completion order is deliberately not a compatibility assertion.
On September 16, Breeze and Chrome 153.0.8010.47 both passed all 12 fixture
assertions. Screenshots were inspected; this is behavioral agreement, not a claim
of pixel-identical font metrics or complete browser compatibility.

Two unmodified upstream dynamic-import WPT files contribute 12 assertions, including
distinct-vs-shared parse, specifier, linking, and evaluation errors. The curated
baseline is 325 files / 2,720 passing assertions, without expected failures.

## Remaining boundaries

Import maps, import attributes, non-JavaScript module types, `data:`/`blob:` module
URLs, source-phase imports, CSP/integrity/Trusted Types integration, and dynamic
imports in worker/synthetic iframe realms remain outside this slice. Unsupported
dynamic-import realms reject explicitly rather than exposing a nonfunctional API.
The parser and dynamic fetch schedulers share compiled records but do not yet
coalesce every overlapping in-flight parser/dynamic request. Parser-prepared
redirect provenance and fully isolated nested-document loading remain follow-ups.
The non-renderer synchronous source-loader helper does not drive this new async
document fetch pipeline. No site-specific source rewriting or security exemption
is used.
