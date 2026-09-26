# CSS nesting and Selectors Level 4 slice

Breeze parses nested style rules and a broader set of selectors as general CSS
features. The [CSS Nesting Level 1](https://drafts.csswg.org/css-nesting-1/)
and [Selectors Level 4](https://drafts.csswg.org/selectors-4/) drafts define the
intended behavior. This work is not an HTML5test score shim; that inventory
does not exercise most of these rules.

## Nested rules

- A nested selector without `&` is relative to its parent as a descendant;
  an initial combinator such as `> .title` remains relative to the parent.
  Explicit `&` composes with the parent selector list and retains its maximum
  specificity, including nonmatching list members. At top level, `&` behaves
  as zero-specificity `:scope`.
- Declarations before and after nested rules retain their original cascade
  positions. Later declarations are represented by nested-declarations rules,
  rather than being incorrectly hoisted ahead of the child rule.
- Nested `@media`, `@supports`, and `@layer` keep the parent selector. At-rule
  keywords are ASCII case-insensitive; longer unknown names are not accepted
  merely because they start with a known keyword. Layer declaration order and
  important reversal follow the separate [cascade-layer contract](css-cascade-layers.md).
- CSSOM exposes child rules of style rules, `CSSNestedDeclarations`, parent
  links, and live declaration/insert/delete mutations. The computed cascade
  reflects those edits without a page reload.
- Balanced braces inside custom-property values are preserved as values, not
  mistaken for nested style rules. Rule depth, stylesheet size, and total rule
  count remain bounded by the engine's hostile-input limits.

## Selectors

- Functional `:is()`, `:where()`, `:not()`, `:has()`, and structural `:nth-*()`
  arguments are parsed as CSS tokens with their specified specificity rules.
  `:is()` and `:where()` use forgiving selector lists; `:not()` does not.
  `:nth-child()` and `:nth-last-child()` accept `of <selector-list>`.
- Identifier and attribute selector tokenization handles CSS escapes and
  quoted delimiters. Attribute `i` and `s` flags are recognized, along with
  [HTML's default case-insensitive values](https://html.spec.whatwg.org/multipage/semantics-other.html#case-sensitivity-of-selectors).
  Invalid trailing tokens reject a selector instead of silently broadening it.
  Empty operands match only with equality, not token or substring operators.
- `:lang()` inherits `lang` or `xml:lang` and uses
  [RFC 4647 extended filtering](https://www.rfc-editor.org/rfc/rfc4647#section-3.3.2)
  for comma-separated language ranges. `:dir(ltr)` and `:dir(rtl)` follow
  [HTML directionality](https://html.spec.whatwg.org/multipage/dom.html#the-dir-attribute)
  from `dir`, including a first-strong-character pass for `dir=auto`.
- `:focus` and `:focus-within` track programmatic and native input focus.
  Focus changes request a render so selector changes are visible, and moving
  a focused subtree updates the old and new ancestor paths.

## Verification

Focused tests cover nested selector matching, specificity, source order,
group rules, custom-property braces, CSSOM edits, escaped tokens, malformed
selector rejection, attribute case rules, language/direction inheritance and
mutation, structural selectors, and focus-driven rendering.

```powershell
cargo test --locked --lib engine::css
cargo test --locked --lib engine::script::tests::cssom_nesting
cargo test --locked --lib engine::script::tests::focus_selectors
```

## Remaining boundaries

This is not a complete Selectors Level 4 or CSS Nesting implementation.
Unsupported pseudo-classes/elements still do not match; selector parsing and
matching retain explicit complexity limits. Nesting beneath shadow-host and
slotted rules requires a fuller composed-tree selector model. Directionality
does not yet account for assigned slot content, and language matching does
not yet use document `Content-Language` fallback or canonicalize deprecated
language tags. The supported CSS property and layout set still limits what a
matching declaration can display. Focus invalidation currently repaints from
the document root because ancestor selector dependencies are not indexed.
