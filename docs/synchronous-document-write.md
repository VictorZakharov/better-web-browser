# Synchronous parser-insertion writes

Updated 2026-09-15. This slice implements re-entry into an **active HTML parser**
from a parser-blocking classic script. It does not implement every dynamic-markup
insertion API or claim a loading speedup.

## Contract and ownership

The [HTML write algorithm](https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#document.write())
and [script end-tag steps](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incdata)
require observable work before `document.write()` returns:

- Written markup is processed by the retained html5ever tokenizer/tree builder.
  Same-script DOM reads and live collections see it immediately. Table insertion,
  split tags, entities, comments, raw text and foreign content keep their parser context.
- Each write stops at its insertion point. Exhaustion of its string is not document
  EOF and does not authorize consuming the unread network tail. Nested writes precede
  the parent's remaining text; paused successive writes retain their order.
- Written inline classic scripts run on the caller's stack in the existing V8 realm.
  `document.currentScript` is saved/restored, errors are reported without aborting
  the writer, and nested return does not perform a premature microtask checkpoint.
- A written external blocking script, or inline script blocked by a written
  stylesheet, pauses tokenization and unwinds to the writer. Existing asynchronous
  fetching, stylesheet dependency/applicability rules, script preparation and
  lifecycle gates resume it later. There is no synchronous network wait inside V8.
- Only stylesheets actually changed by tokenizer input enter that parser-blocking
  set. A DOM-created dynamic link is not silently promoted to a parser-created link.
- DOMString conversion happens before insertion, with the string conversion hint
  and Symbol rejection. `writeln()` includes its newline in the same insertion.
  Parser-created custom-element constructors reject writes with `InvalidStateError`;
  that guard is released even on an exception.

During outer classic execution, the script runtime temporarily owns the parser.
Small native operations parse, register DOM/cache changes, and prepare reached
script elements. Host-state borrows end before custom-element callbacks or author
JavaScript run. Nested classic evaluation uses the active isolate and watchdog,
not recursive entry through the top-level Rust execution helper. Queued script
owners and parser-created sheets return to the document scheduler afterward.

Existing document/node/depth, script-count and aggregate script-byte limits remain.
Write output has a cumulative 8 MiB document budget; nested insertion is capped at
32 levels. Tokenizer feeds remain bounded so a single large write cannot defer
depth enforcement until its end. Runtime termination still unwinds retained input
ownership if V8 skips JavaScript `finally` blocks.

## Verification and reproduction

`tests/live_runtime/parser_writes.rs` exercises the real hidden browser and retained
parser, not the completed-DOM script helper. It covers immediate reads, table tree
building/live collections, conversion, nested current-script and microtask order,
exceptions, recursion limits, custom-element construction, delayed external sources
(including writes from their load handlers), written CSS blocking, and dynamic CSS
not blocking. Parser unit tests cover input ownership, UTF-8/entity chunk boundaries,
and bounded adversarial depth. An additional hidden regression verifies that depth
truncation still releases document lifecycle completion rather than leaving loading hung.

The curated gate adds 60 **unchanged upstream WPT cases** at the existing pinned
revision: `document-write/001.html` through `046.html`, `051.html`, and
`script_001.html` through `script_013.html`. Support scripts stay in the external
BSD-licensed WPT checkout. No expected failures or upstream test edits are added.
Popup/frame, XML, document replacement and module destructive-write cases are not
claimed by this active-parser subset.

Serve `benchmarks/alpha/fixtures/synchronous-parser-writes.html` with
`scripts/serve-alpha-fixtures.ps1` in a hidden process. Its written external source
has a 250 ms response delay. Run Breeze with `scripts/run-hidden-benchmark.ps1`,
`-FreshProfile -SettleMs 1000 -DeviceScaleFactor 1`, a screenshot and diagnostic
selector `#result`. Run the same URL through `benchmarks/chromium`, whose launcher
enforces `--headless`, `--mute-audio` and `CreateNoWindow`. Success requires
`data-contract=pass` and the complete trace, not merely zero exceptions:

`outer → visible → tail absent → inner → child → outer → paused → return → microtask → external → external child → resumed`

The saved pre-change release failed this fixture with zero JavaScript errors:
it reported missing immediate/nested/external children and ran the inline child
after the outer return. Its executable SHA-256 was
`2312D17E85A843C8D9F0CA84A441AE23673A8574A18655877416C5ED8581AF20`.
Headless Chrome 153.0.8010.47 and the new release both passed the exact required trace.
Both final screenshots were inspected: the success panel and inserted content
are present. That run exposed a pre-wrapped trace remaining on one line in Breeze.
The follow-up [preserved wrapping and document-stream slice](document-streams-and-pre-wrap.md)
addresses that difference and records fresh comparison evidence.

Local verification passed: 1,096 library tests (one existing ignored test), 17
WPT-runner tests, all 123 isolated-renderer tests, the complete 53-test live-browser
suite (three existing ignored probes), and all 11 focused write tests after the
final safety changes. The early-scroll timing test failed once in the parallel
live run and passed in the complete serial rerun; no threshold was changed.
The full curated WPT gate passed 292 files / 2,642 assertions with no failures,
timeouts or expected-failure entries. Strict all-target Clippy, formatting and
source-size gates passed. These are correctness checks, not a navigation benchmark.

## Deliberate remaining boundaries

- `document.open()/close()` and writes without an active insertion point now use
  [script-created streams](document-streams-and-pre-wrap.md), replacing the old
  completed-DOM fragment fallback. See that contract's explicit remaining boundaries.
- Parser custom-element notifications still occur at script/input checkpoints,
  not every token. Full reaction timing, parser-originated MutationObserver records,
  callback cleanup boundaries and nested browsing-context/realm loading need
  dedicated slices.
- CSP, Trusted Types, asynchronous module continuation policy and complete stylesheet-set
  selection are not implemented by this change.

This is a generic loading correctness fix, not a YouTube workaround, a visual
parity claim, or evidence of a particular whole-page performance improvement.
