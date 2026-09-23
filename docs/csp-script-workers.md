# CSP3 script and external Worker policies

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

## Observed live boundary

On September 22, 2026, fresh-profile hidden Breeze and unified-headless
Chromium both received Google's HTTP 429 `/sorry/` response to a Google.ca
search from this environment. The server chooses that response; standards
implementation cannot guarantee search results or silently bypass a challenge.
The earlier Breeze capture had an empty challenge frame because its CSP header
was rejected. With this slice, the reCAPTCHA frame's nonced script and
dedicated Worker/imported script progress, but the Worker then needs a
`MessageChannel` with a transferable `MessagePort`. Breeze's Window
MessagePorts are not yet transferable across the Worker thread boundary, so
the challenge is **still not functional**. These live observations are not a
deterministic compatibility test or a claim that the user will see HTTP 429.

Remaining standards work includes cross-agent MessagePort transfer, broader
CSP delivery/reporting and hash handling, and live-site validation without
anti-automation responses. Do not replace these with site-specific response
substitution or challenge evasion.
