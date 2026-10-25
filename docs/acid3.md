# Acid3

[Acid3](http://acid3.acidtests.org/) checks 100 DOM, CSS, HTML, SVG and ECMAScript behaviours and compares the final page with a reference rendering. Breeze targets Firefox's result rather than 100/100, because several subtests check behaviour that later specifications changed (see [Obsolete expectations](#obsolete-expectations)).

## Running it

```powershell
./scripts/test-acid3.ps1 -Browser target/performance/better-web-browser.exe -Firefox -CompareReference
```

- `scripts/fetch-acid3.ps1` downloads the test and its support files into `target/acid3/site` and checks each against the SHA-256 recorded in `scripts/acid3-files.psd1`. The test states no licence, so its files are fetched at run time and never checked in. A hash mismatch means the upstream file changed; review the new file before updating the manifest.
- `scripts/serve-acid3.ps1` serves that copy on loopback with the recorded status codes and content types. These are part of the test: `empty.css` and `support-b.png` are served as `text/html`, `xhtml.1`–`xhtml.3` as `text/xml`, and unknown paths return 404 with a PNG body.
- The server adds one hook after the test loop finishes. It POSTs `score`, `tests.length` and the failure log to `/__report?label=<browser>`, then sets the document title so hidden runs stop as soon as the report is stored. The test itself is unchanged.
- Breeze runs through `scripts/run-hidden-benchmark.ps1`. With `-Firefox`, Firefox runs with `--headless --no-remote` in a throwaway profile and `CreateNoWindow`; only its own process tree is stopped afterwards. The script prints each browser's failing subtests and the subtests that Breeze fails but Firefox passes, and writes `summary.json`.
- `-CompareReference` also renders `reference.html` in Breeze and runs `scripts/compare-acid3-pixels.ps1` over the page area of both screenshots. `reference-diff.png` shows differing pixels in magenta. Breeze's own rendering of the reference is the baseline, so font rasterization cancels out. Expect differences until the score reaches 100/100, since failed subtests change the bucket colours.
- `-MinimumScore` makes the run fail below a given score.

A run takes about 15 seconds (about 25 seconds with `-CompareReference`).

## Obsolete expectations

Firefox 158 scores 96/100 locally and live. Chromium scores 96 locally and 93 live, because live timing subtests are network-sensitive. Firefox's four failures check behaviour that current specifications reversed. Breeze follows the current specifications, so it fails them the same way:

| Subtest | Acid3 expects | Current specification |
| --- | --- | --- |
| 22, 23 | `createElement('di<v')` and `createElementNS(ns, 'di<v')` throw `InvalidCharacterError` | The DOM Standard's [valid element local name](https://dom.spec.whatwg.org/#valid-element-local-name) accepts any name that starts with an ASCII letter and contains no ASCII whitespace, NULL, `/` or `>`. Names that start with `<`, a digit, `-` or `.` still throw, as Acid3 expects. |
| 25 | `createDocumentType('a:', '', '')` throws `NamespaceError` | [`createDocumentType()`](https://dom.spec.whatwg.org/#dom-domimplementation-createdocumenttype) only checks for a [valid doctype name](https://dom.spec.whatwg.org/#valid-doctype-name), which `a:` is. |
| 35 | The root element does not match `:first-child` | Selectors Level 4 [dropped the requirement](https://www.w3.org/TR/selectors-4/#child-index) that the element have a parent, so the root element matches. |

Subtest 38 sets a text child's data to `" "` and expects the parent to stop matching `:empty`. The [Selectors Level 4 draft](https://drafts.csswg.org/selectors-4/#the-empty-pseudo) lets `:empty` ignore document white space, but no engine ships that, so Breeze keeps the interoperable Selectors Level 3 rule that Firefox, Chromium and WebKit implement, and passes.

Reaching 100/100 would mean reverting the four behaviours above to their 2008 definitions. No current engine does that, and it would only serve the test, so Breeze's target is Firefox's 96/100.
