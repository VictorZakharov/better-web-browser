# File API worker reads and Blob byte sequences

This slice follows the [File API](https://w3c.github.io/FileAPI/) for memory-backed
`Blob` and `File` objects. It is a behavioral addition, not an HTML5test score
shim: the current score has not been measured against this branch.

## Worker-only synchronous reads

Dedicated workers expose `FileReaderSync`; Window does not. Its four methods
(`readAsArrayBuffer`, `readAsText`, `readAsDataURL`, and the legacy
`readAsBinaryString`) synchronously consume the engine's private immutable
Blob snapshot. They do not call author-overridable `Blob` methods or accessors.
Incorrect receivers and non-Blob values fail with `TypeError`.

`readAsText` shares the existing FileReader decoding host and its `encoding_rs`
dependency. An explicit encoding label takes precedence over the Blob MIME
`charset`; a byte-order mark then takes precedence according to the Encoding
Standard. Invalid labels fall back to the MIME charset and then UTF-8, matching
the existing asynchronous FileReader path. The host currently limits a text
decode input to 16 MiB. Blob reads still materialize the requested result in
worker memory, so this is not a streaming replacement for large files.
Both reader paths use one private [MIME parser](https://mimesniff.spec.whatwg.org/#parsing-a-mime-type)
for charset extraction. A semicolon inside another quoted parameter cannot
invent a charset, and the first syntactically valid `charset` parameter wins.
For both readers, a Data URL omits its media type when the Blob type is empty.

The asynchronous `FileReader` remains available in both Window and workers.
It now advances through at most 64 KiB of a memory-backed Blob in each queued
read task before yielding to other event-loop work. Its existing `loadstart`,
roughly 50 ms-throttled `progress`, `load`/`error`, `abort`, and `loadend` event
contract remains, while an abort after partial progress reports the actual
`loaded` byte count and cancels subsequent read tasks. Result packaging still
materializes the complete byte sequence on completion; this slice does not
promise constant-memory whole-file reads.
An empty read emits no artificial `progress` event, and a small read that
finishes before the interval also proceeds from `loadstart` to `load` and
`loadend` without one.
No new native dependency, copied implementation, or filesystem privilege was
introduced. A FileReaderSync can read only bytes already in its worker realm's
Blob snapshot; it cannot open an arbitrary local path.

## Blob slices and streams

`Blob.slice()` now applies the File API's clamped byte offsets, including
negative offsets and ties-to-even rounding. A slice references subranges of
private immutable chunks instead of flattening and copying the entire source
Blob. The constructor still copies author-supplied mutable buffers, and public
byte-reading APIs copy before returning data, so a caller cannot mutate a
source or sibling slice through the shared backing store.

`Blob.stream()` now creates a pull-driven readable **byte** stream, allowing
both default and BYOB readers. Default reads copy at most 64 KiB of Blob data
at a time instead of eagerly materializing and enqueuing the whole Blob.
BYOB reads fill the supplied view across internal chunk boundaries. An empty
Blob settles both reader modes; cancellation releases the stream's references
to its source chunks. This uses the project's existing byte-stream machinery
and [Streams Standard](https://streams.spec.whatwg.org/) contract.

The implementation currently holds memory-backed Blob chunks in a realm. It
does not add file-backed streaming, disk snapshots, or worker Blob URL fetches.

## Acceptance

Focused tests cover constructor snapshot isolation, multi-part byte-range
slices, fractional/negative/clamped offsets, independent returned byte arrays,
bounded pull chunks, cross-part BYOB reads, empty-stream completion,
worker-only exposure, all four synchronous read formats, MIME charset and
explicit-label decoding, invalid receiver/input behavior, and preservation of
asynchronous FileReader in workers. Long asynchronous reads are checked for
intermediate `LOADING` state, terminal data integrity, and mid-read abort
without later `load` or duplicate terminal events.
