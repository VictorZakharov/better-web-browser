# HTML event-handler attributes

Implemented 2026-09-15. This is a web-platform behavior slice, not a YouTube patch,
a page-loading speedup, or a claim of full scripting conformance.

## Contract and ownership

The primary contracts are HTML's
[event-handler algorithms](https://html.spec.whatwg.org/multipage/webappapis.html#event-handlers),
DOM's [listener dispatch](https://dom.spec.whatwg.org/#concept-event-listener-invoke), and
Web IDL's [unscopable interface members](https://webidl.spec.whatwg.org/#Unscopable).

- Recognized, no-namespace handler attributes on HTML/SVG elements activate on parsing,
  cloning, fragment insertion, `setAttribute`, namespace-aware mutation, `Attr.value`, and
  `NamedNodeMap` changes. Unrelated attributes and foreign namespaces do not execute.
- A raw attribute reserves one non-capture listener slot immediately. First property access
  or invocation compiles its FunctionBody. Replacement preserves order; removal or assigning
  `null` removes the actual listener record. Reassigning identical content recompiles.
- A syntax error reports once and clears the callback, retaining the slot. Listener failures
  report through the global error event; error-handler failures do not recurse indefinitely.
- Native V8 compilation creates document/form-owner/element object-environment scopes outside
  the private bootstrap closure. Global lexical declarations, strict directives, function
  source/name/arity, and `Symbol.unscopables` remain V8 semantics, not string rewriting.
- Raw handlers in inert documents wait for adoption. Body/frameset Window handlers forward
  to the associated Window, including disconnected elements of an active document.
- Ordinary handlers cancel only for boolean `false`. Window ErrorEvent handlers receive five
  arguments and cancel for boolean `true`. Before-unload callback return conversion and
  `BeforeUnloadEvent` are implemented; navigation confirmation UI is not.
- Retained parsing reconciles attributes merged by later html/body start tags without
  reviving unchanged content after a page has explicitly cleared the IDL handler.

`bootstrap/event_handlers.js` owns the handler map and activation/compilation policy.
`engine/event_handlers.rs` is a small V8 compiler bridge. DOM event propagation remains in
`events.js`. ParentNode/ChildNode mixins and HTML namespace interfaces have separate modules
so the new behavior does not expand already-large DOM coordinators. Node metadata indicates
whether handler attributes exist, avoiding an extra attribute-list host call for every wrapper.

## Verification

The final local run passed 1,091 library tests (one ignored), 17 WPT-runner tests,
43 live-runtime tests (three ignored), and 123 renderer-process tests. The full
curated WPT suite passed 232 cases / 2,582 assertions, including 12 new handler cases /
413 assertions; no failure expectations or upstream test-body edits were added.
Formatting, all-target Clippy and the source-size gate also passed.

Owned unit tests cover lifecycle, scope shadowing, global lexical access without bootstrap
scope leakage, syntax failures, namespace/Attr/NamedNodeMap mutations, cloning/adoption,
custom-element reactions, body/window errors, cancellation and exception reporting.
Hidden live-runtime tests exercise a native trusted click and retained parser attribute merges.

The curated upstream additions cover global handler exposure/dispatch, body load, source text,
replacement/removal ordering, lazy invalid-source compilation and unscopables.
They use the existing pinned WPT revision without editing the upstream test bodies or adding
failure expectations. The local server mirrors upstream's WebIDL parser URL alias, and generated
Window wrappers now create the log/body element before the test script, as upstream does.

The owned `benchmarks/alpha/fixtures/inline-event-handlers.html` fixture and headless
Chrome 153.0.8010.47 both produce:

```text
PASS
body load: true
window listener
markup click: true
default canceled: true
replacement
removed: true
```

Use the hidden alpha fixture server, then `scripts/run-hidden-benchmark.ps1` with a fresh
profile, `-SelectorActivationTarget '#action' -NavigationDelayMs 350 -SettleMs 1600`.
The native activation is delayed until after load for this particular ordering assertion;
first presentation is not itself a load event. Run the Chromium harness with its existing
`--headless`, `--mute-audio`, `CreateNoWindow` launcher and `--click-after-ready 60,50`.
Inspect both screenshots and the result/console trace. These are behavior checks, not
pixel-equality or relative-speed claims. Captures and reports stay under ignored `target`.

## Boundaries

- CSP inline-script policy, Trusted Types, complete sandbox policy, and full nested-frame
  lifecycle/realm isolation are separate work. This slice must not be read as implementing
  those security policies.
- Handler scopes use the existing built-in form-owner model. Form-associated custom elements,
  complete legacy named form properties and all other missing form APIs are not supplied here.
- Syntax diagnostics retain the source document URL and native error, but original HTML
  attribute line/column locations are not retained by the parser.
- EventHandler exposure does not implement an event producer: printing, unload confirmation,
  animations and other not-yet-implemented subsystems do not become functional merely because
  an event-handler property exists.
- The frame-dependent WPT `eventhandler-cancellation.html` was investigated but is not in the
  passing manifest: the browser does not yet expose its required `window.frames` collection.
  Window/element ErrorEvent return behavior is covered by the owned tests.
- `uncompiled_event_handler_with_scripting_disabled.html` likewise requires the not-yet-exposed
  `DOMParser` API. The owned inert-document test uses `createHTMLDocument` to check deferred
  compilation and adoption; it does not claim the missing parser interface is implemented.
- Existing callback-cleanup microtask and broader cross-realm dispatch limitations remain.
  This change does not claim those algorithms are complete.
