# CSS Cascade Level 5 layers

Breeze's stylesheet parser and author cascade implement the layer-ordering slice of
[CSS Cascading and Inheritance Level 5](https://drafts.csswg.org/css-cascade-5/#layering).
This is a general CSS feature, not an HTML5test-specific probe. The capability
score does not exercise it.

## Contract

- `@layer` statement rules establish first-declaration order, including empty
  named layers. Block rules assign their child style rules to a named or unique
  anonymous layer. Dotted names establish nested layers. A parent layer's own
  declarations follow its children for normal cascade order.
- Imported stylesheets retain the importing occurrence's position. `layer(name)`
  and bare `layer` assign imported rules to named and anonymous layers, including
  nested imports. An applicable layered import establishes its layer even if the
  stylesheet fails to load. False media/supports conditions do not establish it.
- Layers are scoped to their cascade origin and shadow-tree context. Normal
  author declarations prefer later layers and unlayered rules; `!important`
  declarations reverse the layer order. Encapsulation context is compared before
  layer, specificity, and source order. Inline declarations retain their
  element-attached precedence within their context.
- `revert-layer` rolls back the supported longhand, shorthand, `all`, or custom
  property to the value below its current layer. Important rollback excludes the
  intervening normal layers and animation origin. Style snapshots and important
  replay run only for an element whose declarations might use the keyword.
- CSSOM exposes `CSSLayerBlockRule` and `CSSLayerStatementRule` with their names,
  grouping-rule children, parent links, and live insert/delete mutation. The
  `CSSMediaRule` and `CSSSupportsRule` grouping path also exposes
  nested layer rules. As for other modern rules, layer rule `type` is `0`; no
  deprecated numeric `CSSRule` constant was invented.

The parser records layer declarations and rule positions independently of its
immutable per-sheet payload cache. Assembly assigns final ranks per stylesheet
occurrence and per shadow context, so two owners of the same CSS bytes do not
share anonymous-layer identity or stale source order after an edit.

## Verification

Focused tests cover order versus specificity, important reversal, nested and
anonymous layers, reopening, conditional rules, imported occurrences and failed
loads, `revert-layer` including custom properties, shadow encapsulation, and
CSSOM mutation of document-owned and constructed stylesheets.

```powershell
cargo test --locked engine::css --lib
cargo test --locked engine::script::tests::cssom_layers --lib
```

## Remaining boundaries

This does not implement the full CSS Cascade 5 surface: user-origin stylesheets,
transition origin, all name-defining at-rules (such as layered `@keyframes`), and
the wider selector/style-rule nesting model remain separate work. The currently
supported CSS properties still bound which declarations can be painted. No
specific HTML5test score increase is expected from layers alone.
