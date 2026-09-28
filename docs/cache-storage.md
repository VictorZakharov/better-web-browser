# Browser-owned CacheStorage slice

This slice implements Window `caches`, `CacheStorage`, and named `Cache` objects on
potentially trustworthy top-level origins. It is a Cache API foundation, **not** a
Service Worker implementation or an HTML5test Service Worker score claim.

The behavior is based on [Service Workers §5, Caches](https://w3c.github.io/ServiceWorker/v1/#cache-objects)
and the [CacheStorage Web Platform Tests](https://github.com/web-platform-tests/wpt/tree/master/service-workers/cache-storage).
`CacheStorage` has `open`, `has`, `keys`, `delete`, and `match`; `Cache` has
`match`, `matchAll`, `keys`, `put`, `add`, `addAll`, and `delete`. Matching ignores
URL fragments, supports `ignoreSearch`, `ignoreMethod`, and `ignoreVary`, and
checks the stored response's `Vary` fields against request headers. A
`CacheStorage.match` without `cacheName` searches named caches in creation
order. `put` rejects non-HTTP(S) or non-GET requests, 206 responses, `Vary: *`,
unsupported response types, and already-used or locked response bodies.
`add`/`addAll` fetch through the normal document Fetch path, require OK responses,
and submit one atomic batch only after every fetch and body read succeeds.
`put` reads a clone, leaving the author's response usable.

The browser resolves every operation from the committed Fetch client's origin;
the renderer cannot select a storage origin in its JSON command. Opaque and
non-trustworthy origins are rejected again in the browser-owned model, even if
a renderer bypasses JavaScript exposure checks. HTTPS and loopback/localhost HTTP
are admitted. The script host currently lacks a complete ancestor secure-context
calculation, so Window `caches` fails closed in **all** embedded frames, including
secure ones. Dedicated-worker `caches` is not yet exposed.

Each origin has a 16 MiB serialized quota; the store has a 64 MiB serialized
quota and individual response bodies are capped at 2 MiB. Storage uses an atomic
profile snapshot with a backup. Loading occurs on the origin-storage worker's
first Cache operation, not on the browser UI startup path. Each mutation still
serializes and commits the bounded snapshot; a database-backed incremental
format will be needed before materially larger quotas. A deleted named cache is
currently not retained by pre-existing `Cache` objects, and HTTP cache modes are
not integrated with CacheStorage. These are explicit remaining conformance gaps.

Coverage includes matching and Vary rules, atomic batch failure, quotas,
origin admission, script-to-model protocol roundtrips, fresh Window and model
reopen, and a hidden browser restart test through the renderer/broker boundary.
