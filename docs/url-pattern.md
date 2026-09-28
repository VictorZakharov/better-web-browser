# URLPattern component matching

Breeze exposes a bounded `URLPattern` subset to Window scripts. It is based on the
[WHATWG URL Pattern Standard](https://urlpattern.spec.whatwg.org/) and uses the
project's [WHATWG URL parser](https://url.spec.whatwg.org/) for URL inputs,
base URLs, default ports, and pathname dot-segment normalization. This is a
behavioral web-platform slice, not an HTML5test score or full conformance claim.

Supported behavior:

- `new URLPattern()` and component dictionaries with `protocol`, `username`,
  `password`, `hostname`, `port`, `pathname`, `search`, `hash`, and `baseURL`.
  Missing components wildcard; a base URL supplies only less-specific omitted
  components. Pattern credentials do not inherit from the base.
- Absolute `scheme://authority/path?search#hash` constructor strings and
  relative pathname strings with an explicit base URL. An origin-only string
  matches any pathname, query, and fragment on that origin. An omitted port in
  an authority string matches the default port, not every port.
- Literal component matching, escaped punctuation, full wildcards, named
  `:part` captures, optional `:part?` segments, and `:part+`/`:part*` repetition.
  A pathname capture preceded by `/` repeats complete slash-prefixed segments;
  hostname repetition stays within a label because `.` is not an automatic
  prefix. A single unmodified named capture may also have literal text on
  either side within its segment, such as `/file-:name.html`. Hostname and
  scheme literals are case-folded; `ignoreCase`
  applies to pathname, search, and hash. Literal internationalized hostnames
  and spaces in paths, queries, and fragments use URL serialization rather
  than raw string comparison.
- `test()` and `exec()` for absolute or base-relative URL strings and component
  dictionaries. `exec()` reports original inputs and per-component input and
  capture groups. Invalid URL input returns `false`/`null`; invalid pattern
  construction throws `TypeError`.

The compiler deliberately rejects syntax it cannot safely match, rather than
silently treating it as a literal or advertising regexp support. Custom regexp
groups, braced groups, duplicate capture names, adjacent captures, multiple
captures within one pathname/hostname segment, and modified captures with
literal affixes are not yet supported. Repetition outside complete pathname
or hostname segments is not supported, nor is repetition in other components.
Each component allows at most one full `*` wildcard; pathname `*` must be
terminal. `hasRegExpGroups` remains `false` for successfully constructed patterns.
Only Window exposure is included in this slice; Worker exposure remains separate
work. Constructor-string parsing does not yet cover opaque-scheme strings or the
complete token grammar; use a component dictionary
for supported patterns that cannot be expressed in a shorthand string. Dynamic
hostname patterns with non-ASCII literal text are rejected until their IDNA
labels can be canonicalized independently of captures.

The implementation caps each pattern component at 4,096 characters, a matched
component at 8,192 characters, and each component at 16 captures. These limits,
and the unambiguous segment-capture grammar, bound regex work on hostile page
input. This is intentionally narrower than the standard's unbounded grammar.

The engine tests cover base inheritance and relative resolution, actual capture
values, escaped literals, query/hash matching, default ports, optional segments,
case options, invalid syntax and input, and adversarial bounds:

```powershell
cargo test --lib engine::script::tests::url_pattern
```
