# Async Clipboard text slice

Secure, active top-level documents expose `navigator.clipboard.readText()` and
`writeText()`. These are real asynchronous requests to the browser-owned Windows
clipboard, not a renderer-local imitation. The implementation follows the
[W3C Clipboard API](https://w3c.github.io/clipboard-apis/) permission boundary:
the renderer sends a bounded intent, while the browser resolves the committed
document client and decides whether OS access is permitted.

A request requires a trustworthy origin (including loopback HTTP), the visible
foreground tab, and a trusted transient user activation. The browser consumes
that activation before use. The first read and first write for an origin each
ask for session-scoped consent in a native dialog with denial as the default;
the decision is held in memory, not persisted. After a prompt's nested event
loop, the browser rechecks tab, document, renderer session, client origin, and
foreground state before touching the OS clipboard. Navigation and renderer
replacement retire outstanding requests. An embedded frame cannot borrow the
top-level tab's activation or permission.

Text is UTF-16 at the Windows `CF_UNICODETEXT` boundary and UTF-8 across the
bounded browser/renderer IPC. The browser allocates the replacement data before
emptying the clipboard so allocation failure does not discard the existing
contents. Requests and replies have size limits; unavailable or non-text data
rejects the promise with a distinct DOM exception. Renderer and browser tests
cover Promise settlement, denial, limits, frame isolation, lifecycle, IPC
roundtrips, and a fake native adapter. Automated tests do not claim to exercise
the actual permission dialog or a physical Windows clipboard session.

This slice does **not** expose `ClipboardItem`, rich-content `read()`/`write()`,
iframe delegation, or persistent clipboard permission. HTML5test's clipboard
row probes `ClipboardEvent`, which Breeze already had, so this work does not
claim a score increase. It uses the repository's existing Windows bindings and
Win32 clipboard APIs; there is no new dependency or copied third-party code.
