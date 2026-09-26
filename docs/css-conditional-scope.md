# Conditional CSS and scoped cascade

This batch treats CSS queries as shared language features rather than as
site-specific branches. The same media-query parser drives `@media`, stylesheet
media attributes, `matchMedia()`, and CSSOM `MediaList`. The same feature-query
evaluator drives `@supports`, `CSS.supports()`, and `CSSSupportsRule.matches`.
That prevents a page from observing one answer in script and a different one
in its applied styles.

## Media Queries Level 4

The parser accepts media types, nested `not`/`and`/`or` conditions, plain,
boolean, and comparison-range features, including reversed and chained
comparisons. Parsing recovers at each top-level comma: one malformed query
serializes as `not all` without discarding a valid neighbor. A syntactically
valid but unknown feature remains in `MediaQueryList.media` and evaluates
`unknown` under three-valued logic, so negation cannot accidentally turn an
unsupported future feature on. `MediaQueryList` change events use the same
evaluation when the viewport changes.

MQ4 §3.2 calls for replacing an unknown-valued whole query with `not all`;
Chromium instead retains its query text in CSSOM. Breeze follows that
serialization for compatibility while preserving MQ4's three-valued matching:
for example, a known true term joined with an unknown one by `or` still
matches. This difference should be revisited if the specification or browser
interoperability settles on replacement.

`MediaList` parses the whole `mediaText` assignment but accepts exactly one
query for `appendMedium()` and `deleteMedium()`. It compares normalized query
serializations, exposes indexed items, and reflects live edits back to its
owning stylesheet or `CSSMediaRule`. `CSSMediaRule.matches` is true only when
its stylesheet is attached to a document with a matching window; a detached
rule remains inspectable but does not match.

The supported media-feature catalog is deliberately narrower than the MQ4
catalog. Unimplemented features evaluate conservatively rather than claiming
support. Physical/viewport/font-relative length units and resolution units
are evaluated where the current media environment has real values; arbitrary
CSS math and a complete device/interaction-feature model remain work.

## CSS Conditional Rules

Feature queries parse boolean nesting without treating a malformed whole
condition as a false value that `not` could invert. General-enclosed syntax
is valid and false, allowing future feature-query forms to be introduced
without invalidating an enclosing rule. `selector()` asks whether the selector
grammar and every required component are actually implemented; it does not
equate forgiving-list recovery with full selector support.

`CSS.supports(conditionText)` accepts a bare declaration with implicit
parentheses. The two-argument `CSS.supports(property, value)` parses a
property value and rejects a trailing `!important`; declaration-form
`@supports (property: value !important)` can parse the annotation. CSSOM
exposes readonly condition text and live `matches` values for both media and
supports rules.

## CSS Cascade 6 `@scope`

The scoped-cascade slice parses explicit start and optional end boundaries,
implicit roots, and nested scopes. Matching uses the actual ancestor path;
selectors inside the scope resolve `:scope`, and an end boundary excludes its
subtree. When competing scoped declarations have equal origin, layer, and
specificity, proximity to the scoping root wins before source order. CSSOM
represents a scope as a `CSSScopeRule` grouping rule and reflects its nullable
`start`/`end` selector lists.

Contiguous direct declarations inside `@scope` are nested-declaration rules
targeting the scope root with zero specificity; runs retain their order among
child rules in both the cascade and CSSOM. CSSOM `insertRule()` also accepts
declaration blocks in nested rule lists and exposes live
`CSSNestedDeclarations` objects, while ordinary top-level grouping rules
reject those declarations. `@import` accepts `scope`,
`scope(...)`, `layer`, and `supports(...)` modifiers in any valid order.
Imported style rules inherit their import occurrence's scope, including
nested imports and repeated uses of one cached URL, but their top-level
selectors do not become relative selectors.

This is not a claim of complete CSS Cascade 6 support. Shadow-host
implicit/`:host` roots and advanced CSS Nesting combinations remain work.
The acceptance tests cover the implemented root/limit/proximity and import
contracts; unsupported edge cases must remain explicit rather than be turned
into apparent support by the feature-query API.

Primary specifications:
[Media Queries Level 4](https://drafts.csswg.org/mediaqueries-4/),
[CSS Conditional Rules Level 3](https://drafts.csswg.org/css-conditional-3/),
[CSS Conditional Rules Level 4](https://drafts.csswg.org/css-conditional-4/),
[CSSOM MediaList](https://drafts.csswg.org/cssom/#the-medialist-interface), and
[CSS Cascade Level 6](https://drafts.csswg.org/css-cascade-6/#scope-atrule),
including [scoped imports](https://drafts.csswg.org/css-cascade-6/#at-import),
and [CSS Nesting Level 1](https://drafts.csswg.org/css-nesting-1/#nested-declarations-rule).
