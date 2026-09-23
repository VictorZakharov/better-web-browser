# CSP3 scripts, Worker ports, and embedded challenge documents

This slice implements a bounded part of [CSP Level 3](https://www.w3.org/TR/CSP3/)
and the [HTML Worker script-fetch algorithms](https://html.spec.whatwg.org/multipage/webappapis.html#fetching-scripts).
It is a web-platform compatibility change, not a Google-specific exception.

- Child response headers admit syntactically supported nonce sources,
  `'strict-dynamic'`, and `'report-sample'`. A matching nonce authorizes a
  parser script; `'strict-dynamic'` suppresses host/scheme and
  `'unsafe-inline'` fallbacks while admitting non-parser-inserted loads.
  Worker imports supply that provenance; generic dynamic DOM-script scheduling
  remains fail-closed until it can prove a trusted loader.
  `report-uri` is recognized as reporting-only, not used to relax enforcement.
  Unsupported source expressions still refuse the policy-bearing response.
- Script-element metadata travels through the renderer request protocol and is
  checked against the browser-owned child response policy before initial and
  redirected requests. Inline execution uses the same nonce check. Missing
  provenance defaults to parser-inserted with no nonce.
- Dedicated Worker entry scripts use `worker-src`, falling back to
  `child-src`, `script-src`, then `default-src`. The browser registers a
  separate client from the final Worker response, including its origin and
  response CSP. Worker imports and Fetch then use that client, not the creator's
  policy. Classic top-level Worker scripts use same-origin mode; classic
  `importScripts()` requests use no-cors mode and non-parser-inserted metadata.
  Internal script bytes may be consumed by the Worker loader but are never
  returned as a page-visible Fetch response.

The owned tests cover the fallback chain, nonce case sensitivity, strict-dynamic
script admission, blocked Worker imports/Fetch, broker-owned Worker policy
registration, and malformed Worker request modes/destinations.

## Worker messages and embedded documents

The next compatibility slices implement bounded Worker-origin `MessageChannel`
and transferable `MessagePort` delivery across the Worker thread boundary,
using the [HTML messaging](https://html.spec.whatwg.org/multipage/web-messaging.html)
and [structured clone](https://html.spec.whatwg.org/multipage/structured-data.html)
algorithms as the contracts. A transferred wrapper is detached in its old
realm; queued messages are delivered through the owning endpoint, not by
sharing JavaScript objects across threads. This covers the Worker-to-Window
transfer used by the observed challenge, not every possible cross-agent
transfer or the complete garbage-collection semantics of MessagePorts.

An [iframe browsing context](https://html.spec.whatwg.org/multipage/iframe-embed-object.html)
now paints its child document in the iframe's replaced-element box, clips it
to that viewport, and routes pointer input back into child-local coordinates.
Child image requests use the normal Fetch/CSP policy and bounded decoder, and
their completion requests a new presentation. For inline replaced images,
[CSS 2.2 relative positioning](https://www.w3.org/TR/CSS22/visuren.html#relative-positioning)
now resolves `left`/`top` percentages against the containing block while
keeping the original image dimensions under overflow clipping. This fixes a
generic large-image tile pattern; it does not special-case reCAPTCHA markup.

## Observed live boundary

On September 22, 2026, fresh-profile hidden Breeze and unified-headless
Chromium both reached Google's `/sorry/` response to the same Google.ca search
from this machine. Breeze observed HTTP 429; the Chromium capture reported the
final challenge document as HTTP 200. The user's regular Chrome did not show
the challenge. Google's exact decision criteria are not observable here:
profile/cookies, browser identity, and automated/headless traffic differ.
The server chooses the response; Breeze must render it, not silently substitute
search results or evade the challenge.

On September 23, the user confirmed that normal Chrome and Breeze reached the
Internet through the same public IP, but a normal Breeze search still received
`/sorry/`. A passive homepage comparison using the same HTTP client and changing
only `User-Agent` received Google's legacy `<input>` form with the Breeze-only
identity and a modern `<textarea>` form with a Chrome-compatible identity.
Google supplied the legacy form's `ie=ISO-8859-1` and empty `biw`/`bih` hidden
fields; Breeze did not add them during submission. A temporary hidden Breeze
capture with a Chrome UA also received the modern form, but exposed a separate
missing CSS Font Loading API (`document.fonts.load`). The browser now advertises
a Chrome-compatible UA with a Breeze product suffix across all sites. That is
content negotiation, not proof that Google will stop challenging searches or
that the modern page is fully compatible; normal-session acceptance remains open.

The earlier Breeze capture had an empty challenge frame. A fresh-profile
hidden replay after these slices showed a live checkbox and, after an automated
checkbox click, a nine-image challenge with distinct clipped tiles. A paired
hidden Chromium capture also showed the image challenge. The challenge was not
solved in automation, and no normal Google search-result acceptance is claimed.
On September 23, a manual Breeze attempt could select image tiles but the
Verify/Skip action did not advance the challenge. This remains an open
compatibility defect; rendering the tile grid is not challenge completion.
Remaining standards work includes broader CSP delivery/reporting and hash
handling, complete MessagePort lifecycle/inter-agent transfer behavior, and
live-site validation when Google serves search results. Do not replace these
with site-specific response substitution or challenge evasion.
