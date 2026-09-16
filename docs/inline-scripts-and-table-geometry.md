# Inline classic scripts and table wrapper geometry

## Synchronous script insertion

Connected, executable HTML classic scripts now prepare and run before the inserting DOM
operation returns. The same V8 realm and watchdog handle nested execution; there is no second
runtime or nested microtask checkpoint. The outer script's currentScript is restored, and a
script in a shadow tree observes null. Exceptions are reported through the global error path
without escaping the inserting call.

Post-connection processing snapshots the inserted subtree after an atomic fragment insertion.
It rechecks connectivity before preparing each script, so an earlier script can remove a later
sibling. Replacement operations finish their removals and mutation records before script
evaluation. Descendant post-connection steps precede the parent's children-changed steps.
Observers receive the normal records and keep their normal microtask delivery.

Empty scripts remain eligible for later child-text changes, including empty scripts from the
network parser. Whitespace/comment-only JavaScript is nonempty and starts the script. Source
uses direct child text, not comment nodes or nested element text. Moving, cloning, or changing
an already-started script does not execute it again. Scripts parsed by innerHTML remain inert;
data blocks and nomodule classics do not execute. Inline async/defer do not postpone execution.
The noModule IDL property reflects its boolean content attribute.

The existing per-source and per-page byte budgets are shared with reentrant execution rather
than duplicated. Parser-written sources still reserved by an active parser session count when
checking nested insertion. The page's bounded prepared-script inventory also bounds recursion.
This does not expand the dynamic module, cross-realm, CSP, or Trusted Types contracts.

Sources: [HTML script preparation](https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element),
[DOM insertion](https://dom.spec.whatwg.org/#concept-node-insert), and
[HTML script cleanup](https://html.spec.whatwg.org/multipage/webappapis.html#clean-up-after-running-script).

The original inline-script-insertion.html fixture matches unified-headless Chrome 153's exact
nested execution, currentScript, inertness, fragment/removal, replacement, and promise-job trace.
Fifteen pinned execution-timing WPT files cover insertion before/after the current script,
movement, empty and nonempty sources, source/type changes, direct child text, script children,
fragments, and inline async/defer. MutationObserver-document.html now passes all four subtests.

## Table wrapper geometry

CSSOM client dimensions now identify table wrappers from computed display, not the HTML tag.
Thus display:table and inline-table use the same wrapper rules as HTML tables; an HTML table
changed to display:block uses ordinary block client geometry. A box removed by display:none
has zero dimensions. Table captions remain part of the wrapper dimensions.

The owned table-scroll-geometry.html fixture checks top/bottom captions, separated/collapsed
borders, CSS-generated tables, client dimensions, enclosing scroll area sizes, positive and
negative clamping, synchronous client-rect translation, and display changes. Chrome and Breeze
agree on its primary data-facts trace, with fractional scroll coordinates compared at 0.01px.
The enclosing-pane scrolling behavior was already implemented; the new correction is the
computed-display-based client geometry, with regression coverage for the full path.

The formerly failing discovery entry table-scroll-props.html already passed on merged main
6944e1d. It is now in the strict gate; it is not counted as a new engine fix. Both former
discovery entries have graduated, and the obsolete failure manifest is retired.

Sources: [CSS table wrappers](https://www.w3.org/TR/CSS22/tables.html#model) and
[CSSOM View scrolling areas and client boxes](https://drafts.csswg.org/cssom-view/).

### Explicit reference differences

Chrome 153.0.8010.47 fails four border-offset assertions in the unmodified pinned
table-client-props.html: it reports inner-table borders where the WPT wrapper contract requires
zero clientLeft/clientTop. Breeze preserves that existing strict WPT contract. In the owned
fixture, the table's own scrollWidth/scrollHeight also differ: for a separated-border table,
Chrome reports 254 x 224 while Breeze's wrapper is 260 x 230. The enclosing scroll container's
extent and offsets agree. The fixture records these differences separately in data-table-areas;
they are not described as Chrome parity or hidden by an expected-failure allowance.

This slice does not complete table overflow painting, row-height distribution, caption/border
painting, border conflict resolution, RTL/vertical scroll origins, or whole-page visual fidelity.
No site-specific branches or dependencies were added.

## Verification

The strict selection is 323 upstream files / 2,708 harness subtests at the pinned WPT revision.
It includes the previous 306-file gate, both graduated discovery cases, and 15 inline-script cases.
Run the normal external WPT checkout and scripts/run-wpt.ps1; no upstream files are copied here.
Focused Rust tests cover synchronous ordering, atomic insertion/replacement, exception delivery,
inertness, shadow/inactive documents, and shared count/byte limits. Hidden production-browser
tests execute both owned fixtures. Reference runs use unified headless Chrome, muted audio, and
100% device scale. This is compatibility evidence, not a loading-performance or whole-spec claim.
