# Session history traversal

The browser owns the session history list. Each entry records its URL, classic history API state,
and the document that created it. A `pushState()` call appends an entry and drops entries after the
current index; `replaceState()` updates the current entry. The renderer updates its own URL and
`history.state` immediately for the calling script, then sends the ordered changes to the browser.
State is copied at the API boundary so later mutation of the original JavaScript object does not
change a stored entry.

Back, Forward, and `history.go(delta)` select an entry in that list. If the target belongs to the
active document, the browser sends a document-scoped traversal input to the existing renderer.
This keeps the DOM, global object, timers, and renderer process alive and does not fetch the URL.
The renderer applies the target URL and state before dispatching `popstate`; if the fragment changed,
it queues `hashchange` afterward with the old and new URLs. `pushState()` and `replaceState()`
dispatch neither event. Going out of range does nothing. A zero delta reloads the current entry.
Only one same-document traversal is in flight at a time. The renderer acknowledges each traversal
after applying it, and the browser defers a subsequent Back or Forward until that acknowledgement.
This preserves the first entry's `popstate` effects before a queued traversal can leave the document.

If the target belongs to another document, the browser uses its document navigation path. The
current implementation refetches that entry; it does not provide a back/forward document cache.
The history index still selects the existing entry, rather than appending a new one.
Entries created by one document remain in the same document group. When a cross-document Back
refetches one of those entries, the browser binds the group's other entries to the new live document,
so a further Back within that group retains the refetched renderer and does not request the URL again.
The selected entry's state and fragment-bearing URL are available before author scripts run; a
fragment is not sent in the HTTP request.
If that refetch redirects, the entry's classic history state is cleared, including when a redirect
chain returns to the original URL. This follows the HTML Standard's history-entry population
steps, which replace the document state and serialize null at the first redirect.

This slice bounds each serialized state to 32 KiB and the tab's retained history to 512 entries.
Each entry also owns its `history.scrollRestoration` mode and saved viewport Y position. The
mode defaults to `auto`, `pushState()` inherits the current mode, and setting an invalid mode
throws rather than silently accepting it. Back/Forward restores a saved CSS-pixel viewport
position for `auto` entries and leaves viewport placement to the page for `manual` entries.
Each same-document history update snapshots the renderer's viewport position when that API call
runs, so a later `scrollTo()` in the same task cannot rewrite an earlier entry's position.
The browser retries an auto restoration while a refetched document grows during streaming,
but stops if the user scrolls or script requests a viewport scroll. A script's scroll request
in a `popstate` handler takes precedence over the browser's saved position. A handler that changes
the target entry to `manual` likewise suppresses automatic restoration; parser-time mode changes
on a refetched document cancel any pending viewport restore.

This is deliberately a **viewport-only** implementation. Nested scrollable regions and child
navigables do not yet have per-entry restoration data. A back/forward document cache and joint
session history across nested browsing contexts also remain separate compatibility work.

The regression coverage in `tests/live_runtime/history_traversal.rs` drives a hidden Breeze run
against a counted loopback server. It checks state restoration, URL and event order, retained
document/process identity, absence of a second request, forward truncation, and the refetch path
for a cross-document Back. It also checks that an out-of-range `go(delta)` does nothing while `go(0)`
reloads the current entry. Renderer-process tests cover the document-scoped input boundary.
`tests/live_runtime/history_ordering.rs` checks that a `pushState()` followed by a full
navigation in the same post-load script task preserves the pushed entry for Back. A navigation
initiated while the document is still loading can replace the current entry under the HTML
Standard's navigation rules, so that case has a different expected history shape.
`tests/live_runtime/history_redirect.rs` exercises a redirect chain that ends at the original
entry URL and verifies that its prior state does not leak into the new document.
`tests/live_runtime/history_queue.rs` checks that two Back calls in one task preserve the first
same-document `popstate` effect before the second Back navigates away. The renderer-process boundary
test asserts the traversal acknowledgement. `tests/live_runtime/history_group.rs` checks that a
refetched document retains its group's remaining entries, and that a fragment-bearing entry restores
both its URL and state on refetch without sending the fragment to the server.
Focused tests check mode inheritance, invalid enum values, protocol bounds, and the mode visible
inside `popstate`.
`tests/live_runtime/history_scroll_restoration.rs` checks saved viewport restoration across
same-document Back/Forward and a cross-document refetch, including `manual` entries that must
not move the viewport automatically. It also checks parser-time updates after a scrolled-away
document, multiple scroll and History calls in one task, and mode changes before automatic
restoration on either a refetch or `popstate`.

Compatibility references:

- [HTML Standard: navigation and session history](https://html.spec.whatwg.org/multipage/browsing-the-web.html)
- [HTML Standard: History API](https://html.spec.whatwg.org/multipage/nav-history-apis.html#the-history-interface)
- [HTML Standard: structured serialization](https://html.spec.whatwg.org/multipage/structured-data.html#structuredserializeforstorage)
