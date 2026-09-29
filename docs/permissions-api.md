# Browser-owned Permissions API subset

Breeze exposes `navigator.permissions.query()` to Window documents for eight
implemented permissions: `notifications`, `geolocation`, `clipboard-read`,
`clipboard-write`, `accelerometer`, `gyroscope`, `magnetometer`, and
`ambient-light-sensor`. The result is an asynchronous
`PermissionStatus` with `name`, `state`, `onchange`, and EventTarget `change` listeners.
The browser reads its existing session-scoped, origin-keyed grant stores; a query never
opens a prompt or grants access. Changes made by the corresponding feature's actual
permission flow update statuses across tabs for that origin.

The browser resolves the request through its registered document client rather than
trusting a renderer-supplied origin. Insecure or opaque origins resolve `denied`. Child
frames also resolve `denied` until Permissions Policy delegation and a browser-owned
frame authority model are implemented; this intentionally denies some legitimate
same-origin frame cases. An inactive child document rejects with `InvalidStateError`.
Clipboard queries reflect only Breeze's browser-owned, per-origin session consent
for its gesture-gated text read/write operations. They never prompt or touch the
OS clipboard; consent or denial from a real clipboard request delivers `change`
to subscribed statuses. The current W3C Clipboard draft specifies the
`clipboard-write` permission and its `allowWithoutGesture` descriptor, but not a
`clipboard-read` query name. Breeze supports the latter as a conservative
extension to expose its separate read grant. Because Breeze always requires an
activation for writing, `clipboard-write` with `allowWithoutGesture: true`
resolves `denied` even after consent to a gesture-gated write. Unsupported names,
including `screen-wake-lock`, reject with `TypeError`. Queries are
limited to 64 per document and 4096 live browser subscriptions overall; overflow
rejects with `QuotaExceededError`, and navigation, renderer replacement, crash, tab
close, or window close retire subscriptions.

This is not complete [W3C Permissions](https://w3c.github.io/permissions/)
conformance. `WorkerNavigator.permissions`, Permissions Policy delegation, persistent
permission controls, and query integration for other supported device APIs remain
follow-up work. A status reflects Breeze's own grant ledger, not a fresh operating-system
permission prompt or a claim that physical sensor hardware is present.
