# Form-control numeric reflection

The script bindings for `<meter>` and `<progress>` implement the HTML numeric
attribute algorithms, rather than treating content attributes as JavaScript
`Number()` expressions.

- Missing, empty, and invalid attributes use the element-specific defaults.
- Content parsing consumes an HTML decimal prefix, skips only ASCII whitespace,
  accepts incomplete exponents as their preceding significand, and normalizes
  negative zero. For example, `2e+` parses as 2 and `0x10` as 0.
- Meter bounds and dependent values are recomputed from content attributes.
  Clamping does not rewrite those attributes. The default optimum is the
  midpoint, calculated without overflowing finite bounds.
- IDL assignments use Web IDL finite `double` conversion. Non-finite values,
  BigInt, and Symbol throw `TypeError` before mutating the attribute. Numeric
  coercion is distinct from content parsing: assigning `"0x10"` stores 16.
- A nonpositive `progress.max` IDL assignment is ignored; a nonpositive `max`
  content attribute instead uses the default maximum of 1.
- Progress is indeterminate only when its `value` attribute is absent. An
  invalid but present value is determinate zero.

This slice changes numeric DOM behavior, not native widget painting, animation,
or accessibility integration. It does not claim complete element conformance
or a particular HTML5test score increase.

Input and textarea `minLength` / `maxLength` share nonnegative `long` reflection.
Missing, invalid, and out-of-range content values read as -1. Content attributes
use HTML integer-prefix parsing (for example, `2e2` reads as 2), while IDL writes
use signed 32-bit conversion before rejecting negative results with
`IndexSizeError`. Failed conversion or range rejection leaves the attribute
unchanged. Reflection bypasses author-overridden attribute methods and retains
mutation records. Native user-edit length validation and select display-size
parsing use the same HTML integer-prefix grammar; programmatic value changes
continue not to trigger user-edit length validity flags.

No dependency or upstream code was added. Decimal conversion reuses V8's
existing `Number()` conversion after selecting the HTML numeric prefix; the
small prefix grammar and element-specific algorithms remain browser bindings.

Primary references:

- [HTML floating-point parsing](https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#rules-for-parsing-floating-point-number-values)
- [HTML numeric reflection](https://html.spec.whatwg.org/multipage/common-dom-interfaces.html#reflecting-content-attributes-in-idl-attributes)
- [Meter](https://html.spec.whatwg.org/multipage/form-elements.html#the-meter-element)
- [Progress](https://html.spec.whatwg.org/multipage/form-elements.html#the-progress-element)
- [Web IDL signed-long conversion](https://webidl.spec.whatwg.org/#es-long)
