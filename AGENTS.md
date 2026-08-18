# Repository instructions

## Open-source quality

- Treat this project as intended for eventual public release. Prefer maintainable, standards-based implementations over site-specific workarounds or benchmark-only behavior.
- Keep changes reviewable: use clear names, focused abstractions, concise comments for non-obvious behavior, and tests that explain the compatibility contract.
- Consult primary specifications and official documentation for web-platform behavior. Record important compatibility decisions in code comments or repository documentation when they are not obvious.
- Do not add dependencies, copied code, generated assets, or vendored material without checking provenance, licensing, and whether an existing project dependency already solves the problem.
- Keep diagnostics and user-facing errors actionable, and do not leave temporary probes, hard-coded test URLs, debug output, or captured artifacts in commits.

## Git and pull-request workflow

- Treat `main` as protected. Never commit or push directly to it, and never bypass branch protection.
- Start each logical change from an up-to-date `origin/main`: fetch, fast-forward local `main`, then create a focused topic branch.
- Keep every pull-request branch linear. Rebase onto `origin/main` when it moves; do not merge `main` or another branch into the topic branch. CI's `Linear PR history` check rejects merge commits introduced by a pull request.
- Keep commits reviewable and scoped to one logical batch. Before pushing, run the relevant local checks plus `./scripts/check-source-size.ps1` and `cargo fmt --all -- --check`.
- Push the topic branch, open a pull request, and wait for all required checks. Only the user may merge pull requests; never run `gh pr merge` or enable auto-merge. Stop at "ready to merge" and provide the pull-request link.
- In this Windows workspace, an escalated `gh pr create` can fail when its internal Git process sees the repository as owned by the sandbox identity. Create PRs with the fully qualified GitHub API endpoint (`gh api repos/VictorZakharov/better-web-browser/pulls --method POST ...`) so no local repository inference is needed; do not mutate the user's global `safe.directory` configuration as a workaround.
- Merge pull requests with GitHub's merge-commit method only. Squash merging and rebase merging are disabled for this repository.
- Preserve the required check names `windows` and `Linear PR history` when editing CI. The `windows` gate aggregates the parallel source/format, lint, and test workers; coordinate any intentional rename with branch protection.
- Markdown-only pull requests may skip the Windows workers only when the checked-in classifier confirms every changed path ends in `.md`. Keep pushes to `main` on the full suite, keep classification fail-safe, and make the required `windows` gate reject inconsistent classifier/worker results.
- Optimize CI for wall-clock feedback. Independent checks may spend additional standard hosted-runner minutes in parallel; do not serialize them merely to minimize total runner usage. Preserve the compiler-level sccache configuration and Cargo registry caches, and measure end-to-end timing after material CI changes. Do not cache `target` directly: checkout timestamps make that ineffective for the vendored Boa path dependency.
- After a merge, delete the remote topic branch, fast-forward local `main` from `origin/main`, and create a new branch before starting unrelated work.
- Preserve unrelated user changes. If the worktree is not clean or the branch cannot be rebased safely, stop and report the conflict instead of stashing, overwriting, or force-pushing.

## Source-file size

- Keep cohesive hand-written source modules at or below 400 lines when practical; 500 lines is the hard limit for new files.
- Run `./scripts/check-source-size.ps1` before committing. Existing oversized files have frozen ceilings and must shrink as they are split; do not raise a ceiling to accommodate new code.
- Keep `src/engine/css.rs`, `src/engine/dom.rs`, `src/engine/layout.rs`, `src/engine/page.rs`, `src/windows_app.rs`, `src/engine/script.rs`, and `src/winhttp.rs` as small facades; extend their responsibility-based submodules instead of moving implementation back into the coordinators.
- Treat files reported above the 400-line target as the next refactor queue. Split high-churn production modules before substantial expansion, and keep low-churn adapters or tests cohesive when a further split would obscure ownership.
- Split by ownership or responsibility, not arbitrary line ranges. Keep a small coordinating module and move tests, platform adapters, bindings, parsing, or lifecycle policy behind focused interfaces.

## Automated browser execution

- Run all browser tests, captures, diagnostics, and benchmarks without visible UI. This applies to Breeze and to every reference browser, including Chromium.
- Never rely on a harness name, prior description, or `Start-Process` behavior as proof that a run is headless. Before running a browser harness, verify the actual browser launch arguments and process settings.
- Chromium reference runs must pass the current unified-headless `--headless` flag and must not create a visible console window (`CreateNoWindow = true` when launched through .NET).
- Breeze runs must use its hidden benchmark/capture modes. Do not launch the interactive browser executable for automated checks.
- Run Breeze page benchmarks through `./scripts/run-hidden-benchmark.ps1`. It sets the executable's fail-closed automation guard, uses `CreateNoWindow`, and verifies `--benchmark` in the actual child command line; do not recreate its launch logic ad hoc.
- If any code path can open a browser window, stop and fix that path before running it. A visible launch is allowed only when the user explicitly requests one.
- Keep repeated and long-running browser checks headless even when a single manual check might seem harmless; unexpected windows disrupt the user's desktop.
- Run `cargo test --test renderer_process` and `cargo test --test live_runtime` as the normal Windows user (an escalated shell in Codex), while preserving their hidden `CREATE_NO_WINDOW` launch paths. The filesystem-sandbox identity cannot access the user's AppContainer profile and produces a false renderer-launch/profile-open failure.
