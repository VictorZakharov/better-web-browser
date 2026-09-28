# StorageManager (default bucket)

Breeze exposes `navigator.storage` in secure top-level documents and dedicated
workers created by secure documents. An embedded document fails closed until the
frame host tracks every ancestor's secure-context status. The object is stable
across reads of `navigator.storage` and exposes `estimate()` and `persisted()`.

The browser, not page script, chooses the committed client's origin for each
`estimate()` request. The existing bounded, ordered database-request broker
does the work off the UI thread and retires requests with their document or
worker. A renderer cannot provide an origin in the request body. The estimate
combines rough accounting from the currently supported default-bucket stores:

| Store | Estimated usage basis | Per-origin cap |
| --- | --- | ---: |
| localStorage | key/value quota units | 5 MiB |
| IndexedDB | serialized per-origin database records | 16 MiB |
| CacheStorage | serialized per-origin cache records | 16 MiB |

Session storage is tab-scoped and excluded. The reported quota is a fixed
37 MiB logical aggregate of these three independent limits. It is not based
on current free disk space, and it is not a reservation or a guarantee that a
write of that size will succeed. Separate per-store and profile-wide limits
still apply. On storage I/O failure, the estimate rejects instead of
misreporting zero usage. A newly created, empty origin reports zero usage.

`persisted()` resolves to `false`: the default bucket retains the Storage
Standard's best-effort mode, and Breeze has no persistent-bucket transition or
permission grant. `persist()` is deliberately absent rather than a method that
pretends to grant durable storage. The implementation does not expose
`usageDetails` or storage-bucket APIs.

This follows the [WHATWG Storage Standard, usage and quota](https://storage.spec.whatwg.org/#usage-and-quota)
and [StorageManager API](https://storage.spec.whatwg.org/#api). Storage estimates
are intentionally approximate; websites must handle quota failures on writes.
