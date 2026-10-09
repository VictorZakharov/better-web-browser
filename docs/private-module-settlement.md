# Private module completion

Pending Window and dedicated-Worker module evaluation stays in native V8
Promise handles. It is never published as a temporary property on an author
global. A bootstrap factory captures the existing bridge before author code,
and the embedder captures and removes that factory from each realm's globals.
Later replacement globals cannot redirect completion or expose native handles.

Capturing the original `Promise.prototype.then` is not enough to observe this
internal promise safely: that method still performs `SpeciesConstructor`, which
can enter author `constructor` and `Symbol.species` getters. A page's Promise
customization must not prevent internal module completion or queued Worker
messages from becoming ready.

The embedder uses the pinned V8 dependency's public `Promise::Then` API via
Rust's `then2`. The bundled V8 15.2.124.1 implementation directly performs the
Promise reaction,
without the public method's species lookup. This is an existing dependency API,
not a patched engine, a polyfill or a special-case interpretation of page code.
The private factory only supplies fulfillment/rejection handlers, not a Promise.
The factory's own array entries are initialized directly and checked before
attachment. No reaction executes in a borrowed HostState or outside the normal
entered-isolate/cancellation guard and microtask checkpoint.

Rejection stays rejection. The captured String intrinsic formats its diagnostic;
if an author's reason cannot be converted to text, a short explicit fallback is
reported instead of abandoning the completion record. This does not make the
module succeed, swallow a host-bridge failure, or hide a renderer cancellation.
A rejected starting Worker retires and does not dispatch its queued messages.
A Window reports the failed module and may continue unrelated script tasks.

Focused tests first reproduced author constructor access in both realm kinds.
They now check constructor/species poisoning, private global replacement,
top-level-await message ordering, rejection, and an unprintable rejection reason.
Window completion is counted exactly once across the standalone helper's
existing startup settlement and later turns. Existing module-lifecycle tests
retain document-load and late-rejection behavior.

The HTML module-evaluation reporting algorithm observes its internal evaluation
promise; it does not require calling a page's public Promise methods. This work
does not alter public `Promise.prototype.then`, subclass species, author awaits,
module fetching, import maps, or script execution budgets.

References: [HTML module execution](https://html.spec.whatwg.org/multipage/webappapis.html#run-a-module-script),
[ECMAScript Promise.prototype.then](https://tc39.es/ecma262/multipage/control-abstraction-objects.html#sec-promise.prototype.then),
and [V8 public Promise API](https://v8.github.io/api/head/classv8_1_1Promise.html).
