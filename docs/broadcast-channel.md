# BroadcastChannel slice

Secure and non-secure top-level documents with tuple origins can exchange
structured-cloned messages through `BroadcastChannel`. The browser process owns
the channel registry and resolves each command against the renderer session,
committed document, and browser-authoritative origin. A renderer supplies a
channel name and serialized message, but cannot choose a recipient tab or
claim another origin. The sender's channel is excluded, while other channels
with the same name and origin receive `message` tasks in admission order. A
closed channel and a retired document receive nothing.

This follows the [HTML broadcasting model](https://html.spec.whatwg.org/multipage/web-messaging.html#broadcasting-to-other-browsing-contexts)
for top-level documents. In that case the top-level site belongs to the
document's own origin, so exact origin matching is also a storage-key match.
Embedded documents need top-level-site/partition authority before they can
participate; exposing the constructor there today would cross a boundary the
browser cannot yet verify. Dedicated workers are likewise not exposed until
their owning client/storage key and lifecycle are carried through this broker.

Messages use the existing structured-clone implementation; transfer lists are
not accepted by this API. Construction requires a name and `postMessage`
requires a value. A post clones the value before the browser admits it, and
mutation after posting cannot change the delivered value. The implementation
captures the trusted `MessageEvent` constructor and dispatch hook before page
scripts run. A listener or `onmessage` handler keeps its channel reachable;
`close()` drops membership and outstanding deliveries for that channel.

The limits are explicit: at most 128 live channels per document, 128 KiB of
serialized content per message, and 512 KiB of synchronous posts per renderer
task. The browser retains one payload per accepted fanout with per-recipient
references; its queues are bounded by recipient count, per-tab depth, and
32 MiB of retained unique payloads. A full destination queue is retried rather
than dropping or reordering accepted messages. An individual post to more than
65,536 recipients exceeds the browser's fanout limit and fails explicitly.
These are resource limits, not a claim that all sizes permitted by HTML are
supported.

Unit tests cover same-origin fanout, sender exclusion, ordering, close and
navigation retirement, quota/backpressure, structured-clone isolation, and
page attempts to replace trusted constructors or dispatchers. A hidden
two-AppContainer renderer test exercises cross-tab delivery through the real
browser/renderer IPC. No new dependency or copied upstream implementation is
introduced; the slice builds on the repository's existing clone, EventTarget,
and renderer-process facilities. HTML5test capability probes alone cannot
establish conformance for messaging, partitioning, or lifecycle behavior.
