# CSSOM View visibility and skipped content

`Element.checkVisibility()` uses the engine's internal box fragments rather than
calling an author-overridable geometry method. By default it reports whether
the element has an associated box; optional opacity and CSS visibility checks
are applied against computed styles. Ancestor opacity is checked through the
composed tree, including shadow hosts. `display:none` descendants and
`display:contents` elements have no box, while the latter's children can.
These decisions follow the [CSSOM View checkVisibility algorithm](https://drafts.csswg.org/cssom-view/#dom-element-checkvisibility).

The `content-visibility: hidden` value is computed and suppresses descendant
block layout and painting while retaining the element's own box. A style change
back to `visible` rebuilds its descendants. This is a partial implementation of
[CSS Containment Level 2](https://drafts.csswg.org/css-contain-2/#content-visibility).
`content-visibility: auto` is deliberately not advertised by `CSS.supports`:
correct auto skipping requires viewport-aware retained sizes and reveal on
scroll, not merely an offscreen test in `checkVisibility()`.

The pinned upstream `checkVisibility.html` diagnostic currently passes 13 of
15 assertions. The two remaining failures are the offscreen
`content-visibility:auto` cases, so that file is not in the strict curated
gate. The owned tests cover box presence, visibility and opacity options,
hidden ancestor suppression, and dynamic reveal without mutating upstream
fixtures.
