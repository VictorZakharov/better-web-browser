# HTML parser conformance fixtures

The `.dat` files use the [web-platform-tests tree-construction
format](https://github.com/web-platform-tests/wpt/blob/master/html/syntax/parsing/resources/README.md).
The test harness compares that stable structural representation with Breeze's engine-owned DOM,
including namespaces, attributes, template contents, fragment contexts, and scripting modes.
Its behavior follows the HTML Living Standard's [tree-construction
rules](https://html.spec.whatwg.org/multipage/parsing.html#tree-construction), [fragment parsing
algorithm](https://html.spec.whatwg.org/multipage/parsing.html#parsing-html-fragments), and
[`noscript` processing modes](https://html.spec.whatwg.org/multipage/scripting.html#the-noscript-element).

`wpt-documents.dat` and `wpt-fragments.dat` contain a deliberately small selection from WPT at
revision `964ddae49acd35592ae4c2a50ea1b9fc2edec686`. The selected cases came from these source blobs:

| Upstream fixture | Blob | Covered behavior |
| --- | --- | --- |
| `blocks.dat` | `a1a9c75218865eaad103a1cbb77263e566868ad2` | implied end tags |
| `tables01.dat` | `40798881618870966bce094d37b5ebd158911952` | foster parenting |
| `adoption02.dat` | `acd388547a1eca204384d2220e5397a4ad865780` | adoption agency |
| `template.dat` | `45fb507c6b9d63564ff423fc8c47e214564a5642` | template contents and template fragment context |
| `namespace-sensitivity.dat` | `050dca752284fbbee72942aa156cc260f784e5c2` | foreign-content integration points |
| `noscript01.dat` | `ec3496ce92fee5878f3455025978c09624cf693a` | scripting-disabled `noscript` |
| `tests_innerHTML_1.dat` | `09e0456f0f12d4564fed2f89e349549d990c163d` | HTML fragment contexts |
| `foreign-fragment.dat` | `e562c6b8ff62c32ccefcf4c4666a1c9dfb45dd85` | SVG and MathML fragment contexts |

Those fixtures retain WPT's BSD-3-Clause terms in `LICENSE-WPT.md`. `local-regressions.dat` is
project-authored coverage derived from the HTML Living Standard's tree-construction, `noscript`,
and duplicate-attribute requirements; it is covered by this repository's MIT license.

The fixture error sections are retained for traceability but are not asserted. Breeze delegates
tokenization and tree construction to html5ever with exact-error reporting disabled; this suite's
contract is the resulting engine-owned DOM structure and its safety invariants, not diagnostic text
or parse-error counts.
