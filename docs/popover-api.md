# Popover API

This implementation follows the [HTML popover algorithms](https://html.spec.whatwg.org/multipage/popover.html) and the [CSS top-layer model](https://www.w3.org/TR/css-position-4/#top-layer) for the supported DOM and renderer paths.

`HTMLElement.popover` reflects the `auto`, `manual`, and `hint` states. The missing attribute has no popover state, the empty value means `auto`, and an invalid token means `manual`. Showing is separate from the attribute: `showPopover()`, `hidePopover()`, and `togglePopover()` update showing state, `:popover-open`, and the top-layer order without rewriting markup. Showing fires cancelable `beforetoggle` before state changes; hiding fires noncancelable `beforetoggle`; `toggle` is queued and coalesced. Invalid state raises `NotSupportedError` or `InvalidStateError` as appropriate.

Auto popovers close unrelated auto popovers, while hint popovers close other hints and manual popovers remain independent. Trusted pointer-down/up outside the retained popover ancestry light-dismiss auto and hint popovers; Escape closes the topmost dismissible popover. Button and button-state input `popovertarget` defaults run after an uncanceled click. An autofocus descendant is focused on show, and hiding restores the prior focus only while focus remains in the popover. Open popovers are laid out separately from normal flow and painted in opening order above page stacking contexts, without ancestor clipping. A `display:none` ancestor still suppresses the top layer.

Current limits are deliberate:

- The CSS length model does not yet represent the `fit-content` keyword. The default open popover uses a scoped intrinsic-width and content-height calculation to approximate the HTML suggested UA stylesheet's shrink-wrapping; authored definite width and height are respected.
- Top-layer style hydration currently covers composed-tree descendants; a popover in unassigned light DOM or a deferred shadow-slot subtree does not yet receive independent style hydration.
- Dialog/popover combined top-layer ordering, `::backdrop`, and general CloseWatcher integration are not covered. Escape handling here applies only to dismissible popovers.
- The explicit `popoverTargetElement` reference uses the same-tree association rule, but does not yet use the specification's weak-reference lifetime machinery.

These limits should be addressed with the underlying CSS/layout, slot-style, and element-reference machinery rather than site-specific special cases.
