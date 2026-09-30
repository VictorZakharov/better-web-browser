# File selection and multipart uploads

Breeze's file input follows the HTML file-upload, form-entry-list, drag-data,
and XHR `FormData` contracts. The page receives `File` objects containing a
snapshot of bytes the user selected; it never receives a local path or an OS
file handle. This is an initial bounded implementation, not a claim of complete
File API or native drag-and-drop conformance.

## Selection boundary

An enabled, connected `<input type="file">` can open the browser-owned Windows
file dialog from a real user activation, including activation through a label,
keyboard, or `showPicker()`. Synthetic script clicks without that activation
cannot open the dialog. The request identifies its document, renderer session,
node, and top-level client; a replaced or background tab cannot accept a stale
reply. Hidden benchmark/capture execution cannot open a native dialog.

The dialog's `accept` attribute is a file-filter hint, not a security or type
guarantee. Breeze allows at most eight selected files totaling 4 MiB, with
each basename at most 255 UTF-8 bytes. It reads each regular file into a
bounded browser-process snapshot after the dialog closes and rejects the
entire selection if any read or limit fails. Only basename, MIME type,
modification time, size, and bounded byte chunks cross into the isolated
renderer. The renderer rejects out-of-order, overlapping, truncated, or
cross-document chunks. There is no stored filesystem access after selection.

`input.files` exposes the selected `FileList`; `input.value` uses the HTML
`C:\fakepath\` prefix and only the first basename. A successful choice fires
trusted bubbling `input` and `change` events in that order; cancellation fires
`cancel` without changing the selection. Reset and type changes clear the
selection. A null `files` assignment leaves it unchanged. `DataTransfer.files`
is a live, same-object list; an event-scoped drag list is copied on assignment
so its later detachment does not erase a dropped file from an input.

## Upload boundary

FormData's entries are private state, not a mutable `__entries` property or an
author-overridable iterator. The `formdata` event receives a provisional list;
`new FormData(form)` clones its post-event entries. `append` and `set` create
File entries from Blobs without asking author-overridden Blob/File getters for
their bytes or metadata. Dedicated workers can construct and send FormData as
well as Window scripts.

For HTTP form navigation, a selected or programmatically created File is
serialized with its actual bytes and MIME type in `multipart/form-data`. The
body is capped at 5 MiB (including boundaries and fields), and pending form
navigation bodies are capped at 10 MiB per document. Oversized bodies fail
before navigation is planned. The private script-to-renderer call uses bounded
base64 rather than a multi-million-element decimal byte array; the renderer
validates again before network dispatch. Fetch and Response body extraction
use the same private FormData entry list. These limits do not assert that all
websites' large-file upload workflows are supported yet.

## Verification

Focused tests cover activation and same-task cancellation, label/keyboard
paths, private callback delivery, FileList liveness, form-entry cloning,
overridden author properties, bounded IPC assembly, and browser-owned fake
file snapshots. Hidden browser loopback tests send byte-exact multipart files
through navigation and a dedicated worker; no automated test opens a visible
browser or OS file dialog.

Primary references: [HTML file upload](https://html.spec.whatwg.org/multipage/input.html#file-upload-state-(type=file)),
[HTML entry lists](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#constructing-the-entry-list),
[HTML drag data](https://html.spec.whatwg.org/multipage/dnd.html#the-datatransfer-interface),
[XHR FormData](https://xhr.spec.whatwg.org/#interface-formdata), and
[File API](https://w3c.github.io/FileAPI/).
