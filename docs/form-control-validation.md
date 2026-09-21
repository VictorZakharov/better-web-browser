# Form control state and constraint validation

This slice owns the form-control vertical path end to end: live control state,
form reset, the constraint-validation API and pseudo-classes, submit gating,
and native invalid feedback. It builds on
[form submission and navigation](form-submission-and-navigation.md).

## Implemented contract

State ownership follows HTML §4.10.5/§4.10.6/§4.10.7/§4.10.10/§4.10.11/§4.10.12/§4.10.15,
validation follows §4.10.18/§4.10.21/§4.10.23, and selectors follow Selectors 4.
The Rust DOM is authoritative for live values, dirtiness, user-edited and
user-validity flags, selectedness, custom messages, and pattern verdicts;
script wrappers delegate through host ops instead of keeping a second store,
and layout, paint, `FormData`, and scripted/scriptless paths all read Rust.

- Values sanitize per state on every write path (seed, scripted, native,
  type transitions). Email/URL strip ASCII whitespace; number/range keep
  strictly valid floats only (`" 123 "` sanitizes to empty); out-of-grammar
  temporal values sanitize to empty. Range values use the bounded midpoint
  default, clamp and round to the nearest allowed step (ties upward), and
  re-sanitize after constraint changes and reset. Reversed bounds use the
  minimum as the default and do not apply maximum clamping.
  Incomplete native numeric edits remain visible but expose an empty value
  plus `badInput`; submission never reuses the previous valid number.
- `willValidate` bars hidden/reset/button states, disabled controls, any
  readonly input (headless Chrome 153: readonly bars every input state,
  including checkbox and color/file/submit), and datalist descendants.
  `<object>` exposes `willValidate` as false for API shape, never a candidate.
- Validity flags report suffering state even on barred controls (disabled and
  readonly inputs still report pattern/type/range/step/custom errors), while
  candidacy gates `checkValidity`/`reportValidity`, `:valid`/`:invalid`,
  submission, and interactive reporting. `valueMissing` additionally requires
  mutability for value-mode and textarea states; checkable, file, and select
  missing ignore barring, and unnamed radios never suffer.
- Radio groups suffer as a unit: every member reports missing when any member
  is required and none is checked.
- Patterns anchor as `^(?:p)$` compiled with the `v` flag; validity is judged
  on the raw pattern (anchoring first can balance a stray paren such as
  `a)(b`). Invalid patterns impose no constraint. Selector matching reuses
  cached verdicts and never re-enters script. Matching uses private V8 realms,
  so author overrides of `RegExp.prototype` cannot alter validity. Scriptless
  matching has the same execution budget as page script; expiration stops the
  document with a diagnostic rather than silently accepting an invalid value.
- Temporal bounds compare as chronological ranks (civil-days date arithmetic,
  ISO week counts, millis-of-day times). Reversed time ranges wrap around
  midnight per the spec reversed-range rule; other states compare linearly.
  Steps use per-state units (days, months, weeks, seconds, seconds) with spec
  default steps (1/1/1/60/60) and bases (1970-01-01, 1970-01, 1970-W01,
  midnight, epoch midnight); absent, unparseable, and non-positive steps fall
  back to the default, and `step=any` never mismatches.
- Reset restores owned controls in tree order, including external `form=`
  controls and detached subtrees. `form.reset()` fires a trusted, cancelable
  `reset` event first; cancellation skips the restore.
- Saved `form.elements`, `fieldset.elements`, `options`, and `selectedOptions`
  collections remain live after tree/selection changes, with indexed, `item()`
  and first-match `namedItem()` access, including detached forms.
- Validation snapshots invalid candidates, then fires trusted, cancelable
  `invalid` events. Cancellation suppresses reporting, **not** invalidity or
  the submission gate. Interactive validation focuses the first unhandled
  failure. See the [HTML static validation algorithm](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#statically-validate-the-constraints).
  Chrome 153 instead skips a later candidate if an earlier invalid handler
  fixes it; this implementation follows the specified snapshot in that case.
- Invalid feedback renders natively (validation bubbles plus accessibility
  reporting) and repaints through the style/paint path with and without script.
  Radio-group changes invalidate each peer's parent, so sibling selectors
  repaint the deselected control as well as the new selection.

## Evidence

- Web Platform Tests at pinned upstream `f9ecd8a4a9c6e9865ea4aee4741e4b02f75fd476`
  (`tests/wpt/manifest.json`, external checkout, `expectations` forbidden):
  **438/438 files pass, 4286/4286 subtests**, including the full
  `pseudo-classes` validity/range/required set and the `constraints`,
  `resetting-a-form`, fieldset, button, radio, and `elements.item()` files.
  Reproduce with `./scripts/checkout-wpt.ps1 -Destination <outside-repo>`
  then `./scripts/run-wpt.ps1 -WptRoot <checkout>`.
- Unit suite: `cargo test --locked --lib` (1355 pass, 1 ignored), including
  review regressions for canceled validation, trusted events, author RegExp
  overrides, range lifecycle, type changes, numeric edits and grammar,
  extreme exponents/years, live collections, and pattern deadline recovery.
- Hidden integration: `cargo test --locked --test renderer_process` and
  `cargo test --locked --test live_runtime` (138/138 and 82 pass/3 ignored).
  The new `form_review` cases cover
  native Enter submission and visible incomplete numbers, with and without
  scripts; the existing radio-label repaint regression is fixed, not waived.
- Owned fixtures under `tests/fixtures/form-validation-*.html` carry a
  `?drive` self-drive mode. Controls/numeric state and submission traces match
  headless Chrome 153; captures are not visually equivalent (see limits).
- Full deterministic alpha comparison: 16/16 fixtures, three runs each, via
  `benchmarks/run-alpha.ps1 -Iterations 3 -SkipBuild`.

### Dense-form review measurements (2026-09-21)

Five measured warm runs after one excluded warm-up, fresh profiles, loopback
500-control fixture, release builds, device scale 1, matched content viewport.
Breeze runs also edit `#c-0-0` through the native benchmark input path. Chrome
is a load-only reference; its timing/memory is not an edit benchmark.

| Measurement | Main `abc344a` | Reviewed PR | Chrome 153 |
| --- | ---: | ---: | ---: |
| Page-ready median / maximum | 325 / 329 ms | 334 / 342 ms | 424 / 429 ms |
| Layout+paint median | 24.9 ms | 25.3 ms | Different counters |
| Process-tree working set median | 33.3 MiB | 33.1 MiB | 555.2 MiB |
| Process-tree private bytes median | 13.4 MiB | 13.1 MiB | 300.9 MiB |

The roughly 3% ready-time difference is below the 15% investigation threshold;
small memory differences are noise, not an optimization claim. Browser ready
milestones and process composition differ (Breeze 2 processes, Chrome 10).
Five samples do not support a useful p95 or long-run leak conclusion. Repeated
edit/reset/teardown memory-growth measurement remains unverified; the native
edit test currently asserts a 10-second upper bound, not Chrome latency parity.
Raw review evidence stays ignored under `target/pr172-review-{wpt.json,alpha,
dense,captures}`; no captured pages or binaries belong in the commit.

## Deliberate limits and remaining work

- Temporal `valueAsDate` reports inapplicability (no date parsing in script);
  temporal stepping applies to validation only.
- Collection liveness is implemented, but specialized `HTMLOptionsCollection`
  indexed setters and `HTMLFormControlsCollection` multi-match `RadioNodeList`
  results remain separate work; passing this curated suite is not full form
  platform conformance. File picker interaction and localized validation UI
  are not implemented by this slice.
- Default checkbox/radio glyphs and native range-slider/spin-button appearance
  are not complete. The missing checkable glyphs were reproduced on main's
  preserved `abc344a` binary as well as this branch. Styled checkable state and
  sibling-selector repaint are tested; that is not a default-widget parity
  claim. The headless captures also retain native UI/outline differences.
- Validity messages stay category-distinguishing English strings; localization
  is out of scope.
