# Repository instructions

## Open-source quality

- Treat this project as intended for eventual public release. Prefer maintainable, standards-based implementations over site-specific workarounds or benchmark-only behavior.
- Keep changes reviewable: use clear names, focused abstractions, concise comments for non-obvious behavior, and tests that explain the compatibility contract.
- Consult primary specifications and official documentation for web-platform behavior. Record important compatibility decisions in code comments or repository documentation when they are not obvious.
- Do not add dependencies, copied code, generated assets, or vendored material without checking provenance, licensing, and whether an existing project dependency already solves the problem.
- Keep diagnostics and user-facing errors actionable, and do not leave temporary probes, hard-coded test URLs, debug output, or captured artifacts in commits.

## Automated browser execution

- Run all browser tests, captures, diagnostics, and benchmarks without visible UI. This applies to Breeze and to every reference browser, including Chromium.
- Never rely on a harness name, prior description, or `Start-Process` behavior as proof that a run is headless. Before running a browser harness, verify the actual browser launch arguments and process settings.
- Chromium reference runs must pass the current unified-headless `--headless` flag and must not create a visible console window (`CreateNoWindow = true` when launched through .NET).
- Breeze runs must use its hidden benchmark/capture modes. Do not launch the interactive browser executable for automated checks.
- If any code path can open a browser window, stop and fix that path before running it. A visible launch is allowed only when the user explicitly requests one.
- Keep repeated and long-running browser checks headless even when a single manual check might seem harmless; unexpected windows disrupt the user's desktop.
