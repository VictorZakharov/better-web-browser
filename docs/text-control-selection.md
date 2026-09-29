# Text-control selection

Breeze keeps the selection of an `input` or `textarea` in document state, even
when that element is detached or has no native Windows edit control. A separate
document-scoped renderer message mirrors it into the Windows `EDIT` control
when one exists. This is distinct from editing the value: changing only the
selection must not dispatch `input` or `change`.

## HTML contract

The selection APIs apply to `textarea` and to `input` in Text, Search, Tel,
URL, and Password states. They do not apply to Email, Number, File, or the
other non-text-selection states. A non-applicable input reports `null` from
`selectionStart`, `selectionEnd`, and `selectionDirection`; the corresponding
setters, `setSelectionRange()`, and `setRangeText()` throw
`InvalidStateError`. `select()` has no effect in these states.

Offsets are UTF-16 code units, not bytes or Unicode scalar values. A new
control starts with a collapsed cursor at zero. `setSelectionRange()` clamps
to the relevant value and collapses at `end` when `end <= start`; a changed
range or direction queues a bubbling `select` event. `setRangeText()` replaces
the specified range and implements `select`, `start`, `end`, and `preserve`
selection modes. Its range coordinates for `textarea` use the LF-normalized
API value, so CRLF in the raw value counts as one line break there. Scripted
selection changes never synthesize a text edit.

The legacy `execCommand()` text-editing path requires exposed selection
offsets. It does not edit a focused Email-state input; attempting an insert or
delete there returns false without first changing the value. This is separate
from ordinary native editing of that control.

## Native control boundary

Windows `EDIT` indexes text in UTF-16 as well, but its multiline text uses
CRLF. Breeze maps offsets in both directions when forwarding native textarea
selections or applying script-origin ranges. The browser only accepts a
selection message for the active document and native text control. Each
renderer-origin mirror includes the latest input sequence it observed; the
browser rejects a mirror older than a newer user edit or accepted mirror, and
rejects a claimed input sequence it has never issued. The mirror includes the
private DOM value so `setRangeText()`, value setters, and form reset synchronize
native text before applying the range. Updating the Windows edit suppresses
native change notifications rather than reporting the script edit as user input.
Value snapshots and their pending aggregate are limited to 64 KiB; larger
script values keep their DOM semantics but cannot use this native mirror. A script may set a
range before its native control exists, so the browser retains a bounded
latest-per-node mirror until a matching control is projected.

`EM_SETSEL` can put the caret at either end of the range, so script-origin
backward selections remain backward in the native edit control and survive
control recreation. `EM_GETSEL` exposes sorted endpoints only. Native
mouse/keyboard changes therefore report direction `none` where the active
end cannot be recovered reliably; this is a remaining direction-parity gap,
not a claim of complete Windows text-selection behavior. Document selection,
IME/composition, and full editing-command interoperability remain separate
work.

## Verification

Focused tests cover applicability, initial and detached state, UTF-16
offsets, clamping, `setRangeText()` modes, events, textarea newline mapping,
typed IPC validation, stale document/input rejection, and native range
preservation. Automated browser tests use only the hidden renderer paths.

Primary references: [HTML text-control selection](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#textFieldSelection),
[Selection API events](https://w3c.github.io/selection-api/#selectionchange-event),
[Win32 `EM_GETSEL`](https://learn.microsoft.com/en-us/windows/win32/controls/em-getsel),
and [Win32 `EM_SETSEL`](https://learn.microsoft.com/en-us/windows/win32/controls/em-setsel).
