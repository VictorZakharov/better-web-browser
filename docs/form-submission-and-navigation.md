# Form submission and navigation

This is the historical PR #169 report. Its iframe blocker is addressed by the
[subsequent child-context/navigation slice](iframe-browsing-contexts.md), including
an owned relay → Back → second-search test and three successful fresh release live runs.
The measurements below remain the original form-slice evidence, not current iframe
limitations. The later [modern-search acceptance](modern-search-acceptance.md)
records retirement of the HTML fallback.

This slice follows [the collections/readiness assessment](browser-performance-readiness-2026-09-19.md).
It implements the missing native form-navigation path and diagnoses the separate modern
DuckDuckGo result-link failure. At that checkpoint it did **not** complete
modern-search acceptance; issue #89 remained open.

## Implemented contract

The implementation follows the [HTML form-submission algorithm](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#form-submission-algorithm),
[entry-list construction](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#constructing-the-entry-list),
and [input type reflection](https://html.spec.whatwg.org/multipage/input.html#attr-input-type).

- `form.submit()` bypasses constraint validation and the `submit` event.
  `requestSubmit(submitter)` checks the submitter's type/ownership, runs existing
  constraint validation unless bypassed, and dispatches a trusted, cancelable
  `SubmitEvent` with its submitter. A canceled event does not navigate.
- Submit-button activation and implicit Enter submission use the same DOM path.
  Missing or invalid input `type` values expose the standard `text` IDL value,
  without rewriting the content attribute. This matters for controlled inputs:
  libraries use that value to decide whether to track text edits.
- Entry lists are constructed after submit listeners, in associated-control tree
  order, including external and hidden controls. Disabled controls (with the first
  fieldset-legend exception), unchecked radio/checkboxes, unchosen buttons, and
  disabled options are omitted. Enabled selected options and repeated names remain.
  `formdata` listeners can change the entry list before serialization.
- HTTP(S) GET replaces the action's query and preserves its fragment. POST carries
  UTF-8 URL-encoded, plain-text, or multipart bytes through renderer IPC to the existing
  browser-owned Fetch/redirect pipeline. Newlines are normalized for submission;
  submitter action/method/encoding/target overrides are honored.
- Submission plans are asynchronous and replace an earlier plan for the same form.
  Removing a form during `submit` or `formdata` prevents submission; removing it in
  a later microtask does not cancel an already planned navigation. Native task and
  FormData references do not call author replacements of those public globals.
- URL, body, target, referrer suppression, history intent, and trusted-input provenance
  travel together. Coalescing cannot attach an old POST body to a later URL.
  The broker validates body/header/target bounds and keeps scripted redirect limits.
- Uncanceled primary synthetic anchor clicks run the generic navigation default,
  including detached anchors. Canceling an original link and activating a new anchor
  works without site-specific rules. Trusted link hit testing, fragment scrolling,
  modifier keys, and tab disposition remain renderer-owned.

The IPC major version changes to 13 for the new navigation payload. Browser and
renderer must be built together; mixed protocol versions are rejected.

## Boundaries — not full form or iframe conformance

- The transport deliberately caps a form POST body at 128 KiB and pending form
  plans at 32. Exceeding a bound reports an error; it is not silently changed to GET.
- Only HTTP(S) submission is supported here. Top-level `_self`, `_parent`, `_top`,
  and `_blank` targets are supported; arbitrary named browsing contexts are not.
  Submission uses UTF-8, not the full legacy document-encoding selection algorithm.
- Existing constraint validation, select selectedness/dirty state, file-picker
  integration, directionality, image-submit coordinates, and form-associated custom
  elements are not complete. The tests establish the listed behaviors, not all
  control states. Native image submission currently supplies coordinates `0,0`.
- Session history stores URLs, not replayable POST requests: POST reload/history
  resubmission UI is not implemented. The tested Back flow uses GET documents.
- Iframe URL loading, child script execution in separate globals, and the complete
  nested-navigation/origin/sandbox lifecycle are not implemented. Existing inert
  iframe document exposure must not be mistaken for a functioning child browser.
- Legacy non-parser/scriptless input paths still have their older GET fallback;
  live HTML documents use the DOM submission path introduced here.

## Result-link diagnosis

In the inspected live modern DDG result interaction, the page's handler cancels
the original anchor click, creates an iframe pointing to `/post3.html`, and uses
an iframe message relay before navigation. Breeze does not load and execute that
child URL, so the relay cannot finish. Following the original canceled anchor or
special-casing that endpoint would hide the missing platform behavior.

An original loopback fixture reproduces the dependency without copying the site's
code: create a same-origin iframe, execute its script, exchange origin/source-checked
messages, then navigate the parent. Headless Chrome reaches the destination;
Breeze remains on the starting page. The separate synthetic-anchor fixture passes
in both browsers. This narrows the observed failure to nested-context execution,
not an unconditional failure to follow links.

The next slice needs real child-document loading, separate globals and lifetime,
origin/sandbox enforcement, and message/navigation integration. It must be verified
with owned and upstream iframe tests before retrying live result activation.
Executing iframe scripts in the parent realm is not an acceptable shortcut.

## Reproduction and acceptance

September 19 validation, comparing preserved merged #168 (`81a0d16`) with this release:

| Check | Before | After | Headless Chrome |
|---|---:|---:|---:|
| Nine directly comparable owned form/link cases | 0/9 | 9/9 | 9/9 |
| Result → Back → second Unicode search | Old harness lacks Back action | Pass | Pass |
| Curated WPT | 383 files / 3,372 assertions | 385 files / 3,380 assertions | Not run by this runner |
| Existing owned visual/performance gate | 48 pairs pass (#168) | 48 pairs pass, unchanged ceilings | Paired reference |
| Owned iframe relay diagnostic | Not measured | Fail: stays on source page | Pass: reaches destination |

The nine-case baseline includes console-reported exceptions: missing form methods
do not count as a correct cancellation merely because the URL stayed unchanged.
The full final default matrix is **10/10** in both engines.

The final Breeze library suite passes 1,178 tests (one existing ignore), browser
unit tests 115, WPT-runner unit tests 17, renderer integration 129, and live-runtime
integration 82 (three existing live-service ignores). Clippy with warnings denied,
formatting, and source-size validation pass without raising file-size ceilings.
The final release also passes all 48 existing alpha fixture pairs (16 fixtures,
three fresh-profile pairs each), including the long-form early-scroll gates.

Three fresh hidden release DDG runs change `web standards` to `wikipedia web browser`
through native control input and Enter. All three reach the new query with the
correct title, ten result-title links, and no Breeze JavaScript/console errors.
The final screenshot retains the search component and populated results.
The fresh headless Chrome comparison receives a challenge and no results, so it
does **not** provide an equal-content live interaction or performance comparison.
No challenge bypass was attempted. Live result activation remains unaccepted for
the iframe reason above.

All browser runs are hidden and muted, with fresh temporary profiles. Breeze uses
the fail-closed benchmark wrapper; Chromium uses unified `--headless`, `--mute-audio`,
and `CreateNoWindow`. No user profile or browser API override is needed.

```powershell
./scripts/prepare-v8.ps1 -Profile release
cargo build --release --locked --bin better-web-browser --bin wpt-runner
dotnet build benchmarks/chromium -c Release
./scripts/test-form-navigation.ps1 -OutputDirectory target/forms-breeze
./scripts/test-form-navigation.ps1 -Chrome -OutputDirectory target/forms-chrome
./scripts/run-wpt.ps1 -WptRoot ../wpt -SkipBuild -Jobs 4
./benchmarks/run-alpha.ps1 -SkipBuild -Iterations 3
```

The ten default owned cases cover direct submission, three POST encodings, a 303
POST-to-GET redirect, cancellation, removal during submission versus after planning,
synthetic link replacement, and result → Back → native input → Enter with a second
Unicode query. Acceptance checks observed server methods/headers/bodies and final
URLs, plus JavaScript and console-reported errors; an empty uncaught-error array
alone is insufficient. The multi-action harness waits for every queued action,
including a navigation already scheduled when the preceding settle timer expires.

At PR #169 the extra iframe diagnostic failed in Breeze and was not part of the
passing default form suite. It now passes without weakening its assertion; the
current default `flow` fixture also navigates through a child-message relay before
Back and the second search. Run the explicit relay cases with:

```powershell
./scripts/test-form-navigation.ps1 -Cases iframe,iframe-external -OutputDirectory target/iframe-breeze
./scripts/test-form-navigation.ps1 -Chrome -Cases iframe,iframe-external -OutputDirectory target/iframe-chrome
```

The two added, unmodified upstream WPT files test `SubmitEvent` and `FormDataEvent`
construction (eight assertions). They do not establish full form-navigation
conformance; actual broker/navigation behavior is covered by the owned server tests.
Upstream files stay in the pinned external WPT checkout under its existing BSD license.

Evidence is ignored under `target/forms-navigation-proof`; third-party captures and
temporary diagnostic probes are not committed. The preceding performance assessment
remains historical: this functional slice makes no new startup, load-time, or memory claim.
