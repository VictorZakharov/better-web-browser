# Detached HTML and XML documents

`DOMParser.parseFromString` now creates independent engine-owned documents for
the five MIME types in [HTML's DOMParser interface](https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#the-domparser-interface).
It is not a site-specific substitute or a string-search parser.

## Contract

- HTML uses the existing html5ever tree builder with scripting disabled. Parsed
  scripts remain inert after adoption/import; autonomous custom elements upgrade
  when inserted into the live document, not during detached parsing.
- XML retains qualified names, namespaces (including explicit, redundant `xmlns`
  attributes), text, CDATA, comments, processing instructions, doctypes, and
  internal entity text. Ill-formed input produces an XML `parsererror` document.
- Parsed documents have their own content type, compatibility mode, and a snapshot
  of the calling document's URL. Their encoding is UTF-8 regardless of declarations
  in the supplied string; a leading Unicode byte-order mark is consumed. They have
  no window or location and do not read/write the
  active page's cookies: HTML defines documents without a browsing context as
  [cookie-averse](https://html.spec.whatwg.org/multipage/dom.html#cookie-averse-document-object).
- Node ownership, cloning/import/adoption, mutable XML character data, and adopted
  SVG geometry and HTML CDATA rendering use the existing DOM and rendering paths. CDATA is represented
  as its own node kind, not flattened into ordinary text.
- XHR document responses share the parser but follow their separate
  [response-document contract](https://xhr.spec.whatwg.org/#document-response):
  response URL, MIME essence and XML suffix handling, HTML only for explicit
  `responseType = "document"`, and null for XML parse failures. Replacing the public
  DOMParser constructor cannot alter XHR's internal parsing. A valid document whose
  root is named `parsererror` is not confused with a parse failure.

## Dependency and safety decisions

The existing HTML parser is reused. The existing transitive roxmltree dependency
discards distinctions needed by the mutable DOM, including CDATA and explicit
namespace-declaration identity, so it is not used to construct these documents.

- [`xml` 1.4.0](https://github.com/kornelski/xml-rs), MIT: streaming semantic XML
  parsing, namespace resolution, entity expansion, and doctype identifiers.
- [`xmlparser` 0.13.6](https://github.com/RazrFalcon/xmlparser), MIT OR Apache-2.0:
  lexical start-tag spans and explicit namespace declarations. The semantic
  reader exposes in-scope namespaces, which alone cannot distinguish inherited
  declarations from redundant declarations actually present on an element.

Both are registry dependencies pinned in Cargo.lock; no upstream source is copied
or vendored. The XML parser does not fetch external entities. Input/expanded-text,
node-count, depth, and entity-recursion limits are enforced in addition to the
realm's retained-node budget. These limits are hostile-input policy, not a claim
of unrestricted XML conformance.

Namespace source-position lookup advances from the previous position on long
minified lines rather than rescanning each line prefix for every element.

## Evidence and remaining scope

Eight unmodified upstream WPT files add 69 passing assertions for HTML/XML parsing,
encoding, doctypes, parser errors, stylesheet access, and attribute preservation.
The pinned upstream revision and case list are in `tests/wpt/manifest.json`.
Owned runtime tests additionally cover inert scripts, custom-element insertion,
URL snapshots/base resolution, namespace declarations, mutable character data,
XHR response identity/MIME/error handling, and bounded XML failure. The original
`dom-parser` alpha fixture imports parsed HTML and SVG into a visible document and
requires completion in both Breeze and headless Chrome.

This is not complete XML-platform coverage. XMLSerializer and XML-specific
innerHTML serialization, full DTD validation/default attributes, cross-realm iframe
DOMParser behavior, and Trusted Types enforcement are not supplied by this slice.
XHR's existing byte-to-text decoding is unchanged; the string parser's UTF-8
contract is not a claim of complete XHR charset support. Namespace declarations
inside entity-expanded markup use the semantic reader's namespace-difference
fallback, which cannot preserve redundant declarations from entity replacement
text. Modern-site acceptance remains separate from these platform tests.

The initial fixture also exposed a pre-existing SVG renderer gap: SVG `<text>` is
not painted (`resvg` text support is disabled). Its broad image-difference score
passed despite the missing label, so that score was not accepted as proof of text
fidelity. The final fixture separates imported SVG geometry from CDATA imported
into an HTML paragraph; a renderer-process regression requires the CDATA text in
the actual display list. SVG text rendering remains unsupported, not silently
counted as parser coverage.
