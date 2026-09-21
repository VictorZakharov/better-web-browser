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
  temporal values sanitize to empty; range values clamp to their bounds
  (spec defaults 0/100, reversed bounds settle on the maximum).
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
  cached verdicts and never re-enters script.
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
- `form.elements` and `fieldset.elements` support indexed `item()` access.
- Submit gating blocks on the first invalid control, reports and focuses it,
  and fires cancelable `invalid` events at each failing control for both
  `checkValidity` and `reportValidity`.
- Invalid feedback renders natively (validation bubbles plus accessibility
  reporting) and repaints through the style/paint path with and without script.

## Evidence

- Web Platform Tests at pinned upstream `f9ecd8a4a9c6e9865ea4aee4741e4b02f75fd476`
  (`tests/wpt/manifest.json`, external checkout, `expectations` forbidden):
  **438/438 files pass, 4286/4286 subtests**, including the full
  `pseudo-classes` validity/range/required set and the `constraints`,
  `resetting-a-form`, fieldset, button, radio, and `elements.item()` files.
  Reproduce with `./scripts/checkout-wpt.ps1 -Destination <outside-repo>`
  then `./scripts/run-wpt.ps1 -WptRoot <checkout>`.
- Unit suites: `cargo test --locked --lib` (1345 pass, temporal/step/range/
  selector contracts included), `cargo test --locked --test renderer_process
  -- form_validation` (6 hidden native tests: submit gating, select/checkable/
  radio/reset, pattern/numeric/badInput, native repaint, dense presentation),
  `cargo test --locked --test live_runtime` (82 pass).
- Owned fixtures under `tests/fixtures/form-validation-*.html` carry a
  `?drive` self-drive mode with headless Chrome 153 reference outputs.
- Dense presentation (400+ controls), hidden release run:
  `Scripts/run-hidden-benchmark.ps1` against a static serve of
  `tests/fixtures/form-validation-dense.html` reports page-ready 579 ms with
  21 ms layout+paint; the native-edit test asserts presentation under 10 s.
  Captures regenerate under ignored `target/` (for example
  `target/form-validation-dense.png`) and are not committed.

## Deliberate limits and remaining work

- Temporal `valueAsDate` reports inapplicability (no date parsing in script);
  temporal stepping applies to validation only.
- `tests/renderer_process/checkable.rs::radio_label_click_repaints_live_selection_with_and_without_javascript`
  fails identically on the base commit (environmental, unrelated to this slice).
- Validity messages stay category-distinguishing English strings; localization
  is out of scope.
