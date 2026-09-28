# Browser-owned custom protocol handlers

The secure top-level `Navigator` exposes `registerProtocolHandler(scheme, url)` and
`unregisterProtocolHandler(scheme, url)` as a bounded implementation of
[HTML §8.10.1.4](https://html.spec.whatwg.org/multipage/system-state.html#custom-handlers).
The renderer checks the required `%s` placeholder, ASCII-lowercases the scheme,
admits only the specification's safelist or `web+` followed by ASCII letters,
parses the URL relative to the document, and requires an HTTP(S) template on
the document's origin. The browser repeats these checks against the committed
top-level Fetch client; a renderer-supplied origin cannot grant permission.
Missing placeholders and URL parse failures are `SyntaxError`; prohibited
schemes, non-HTTP(S) templates, and cross-origin templates are `SecurityError`.
The URL argument is converted as Web IDL `USVString`. This browser's anti-abuse
policy caps schemes at 32 bytes and template URLs at 2 KiB; oversized schemes
fail with `SecurityError` before an IPC request is sent.

Registration is an intent, not an automatic grant. A visible, foreground,
non-minimized browser asks for explicit consent only after a trusted user
gesture. That gesture is consumed before the native prompt so a page cannot
queue a train of prompts. The browser rechecks tab, document, renderer session,
and origin after the modal loop before saving a grant. Denials are remembered to
avoid repeat prompts. Only one accepted handler is active per scheme, and
approval never changes Windows' default applications. The profile store is
bounded and replaced from a synced same-directory temporary file; malformed or
ambiguous grants fail closed. Breeze holds an exclusive profile lock for the
browser process lifetime, preventing another process with a stale registry from
restoring a revoked grant. A second process must use a different profile or wait
until the first exits. `unregisterProtocolHandler` removes only the calling
origin's matching grant.

Trusted and synthetic anchor navigation, and address-bar navigation, can pass
eligible custom schemes to the browser boundary; Fetch and subresource loading
still reject them. Only an approved handler routes. Routing strips embedded URL
credentials, UTF-8 component-encodes the serialized input URL, substitutes its
first `%s`, and navigates to the resulting web URL. Unknown schemes leave the
current page intact, including `target="_blank"` links (no empty tab is opened).
This is not operating-system protocol registration, and
there is not yet a management UI for editing saved choices. Embedded frames and
headless runs do not prompt. The API's presence alone is not a conformance or
HTML5test score claim; a measured score is recorded only after a fresh hidden
release capture.

The implementation reuses the repository's `url` and `serde_json` dependencies
and Windows' native dialog API; it adds no copied browser code or new dependency.
Tests cover the parameter and error contract, origin admission, profile reopen,
approved-only routing, credentials stripping, IPC, script binding, and hidden
renderer anchor activation.
