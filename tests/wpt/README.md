# Curated Web Platform Tests

This directory defines Breeze's small upstream conformance regression suite. Test files are never
copied into this repository. The runner requires a separate sparse checkout of the official
[web-platform-tests/wpt](https://github.com/web-platform-tests/wpt) repository at the exact revision
recorded in `manifest.json`.

WPT is distributed under the
[3-Clause BSD License](https://github.com/web-platform-tests/wpt/blob/f9ecd8a4a9c6e9865ea4aee4741e4b02f75fd476/LICENSE.md). The
external checkout retains the upstream source, history, metadata, and license. Breeze's
`reporter.js` is an original adapter which uses testharness.js's documented result and completion
callbacks; it replaces `/resources/testharnessreport.js` only for these hidden local runs.

## Prepare fixtures once

Choose a location outside this repository:

```powershell
.\scripts\checkout-wpt.ps1 -Destination ..\wpt
```

The setup script sparsely checks out only `resources/testharness.js` and the paths in the manifest.
It refuses to place upstream fixtures inside the Breeze worktree or overwrite a dirty checkout.
Network access is needed only to create or update this external checkout.

## Run the suite

After the fixtures exist, all 30 curated cases run with one offline command:

```powershell
.\scripts\run-wpt.ps1 -WptRoot ..\wpt
```

Use `-Filter "DOM events"` for a single area or a path substring. Use `-SkipBuild` only after both
release binaries are current. `-Jobs 4` runs four isolated hidden processes concurrently. The
command starts a loopback-only static server and launches a fresh hidden Breeze benchmark process
per case. JavaScript `.any.js` and `.window.js` files run in a generated Window test page. This first
curated set intentionally avoids WPT server substitutions, special hostnames, and support files;
expanding beyond that contract should use the official WPT server rather than ad-hoc emulation.

CI passes `-BuildProfile debug -Jobs 4`: conformance is not a performance measurement, and benchmark
processes launched by this runner request an explicitly sized headless UI-thread stack so large
standards harnesses remain safe in unoptimized builds. Normal local runs remain sequential release
builds, and unrelated performance benchmarks keep the browser's ordinary main-thread path.

The console reports each case and `target/wpt/report.json` records the pinned revision, harness
subtests, JavaScript diagnostics, durations, and one of four actual outcomes: `pass`, `fail`,
`timeout`, or `crash`. Expected non-passes require a reason in the manifest. A matching expected
failure is successful, while an unexpected pass, changed failure mode, regression, or crash makes
the command fail. The current pinned baseline is 30 passes with no expected failures or timeouts.
This forces the manifest to be updated deliberately when compatibility changes.

The runner follows the official
[testharness.js API](https://web-platform-tests.org/writing-tests/testharness-api.html). It is a
focused regression gate, not a replacement for WPT's full
[local runner and server](https://web-platform-tests.org/running-tests/from-local-system.html).
