# Repository instructions

## Daily work boundary

- Wrap up work by 9:00 p.m. in the user's local time zone (`America/Toronto`) each day so the user can shut down their computer. Plan checks, builds, pushes, and handoff with enough margin; do not start a new long-running task near the cutoff.
- If a task cannot be completed by then, leave the branch and worktree in a recoverable state, report what is done and what remains, and continue only when the user resumes work on a later day. Do not keep running checks or background work past the cutoff without the user's explicit exception.

## Local artifact storage

- On this machine, keep generated build outputs, benchmark profiles/captures, and task scratch files on the G: drive. Before running Cargo or benchmark scripts from a C: worktree, set `CARGO_TARGET_DIR`, `TEMP`, and `TMP` to task-specific directories on G:. Do not create a `target` directory in a C: worktree.

## Local build environment

- Native dependencies (`mozangle` via bindgen, V8) need LLVM 19+ `libclang.dll` with its matching `clang.exe`, and CMake. A plain `cargo build` without them fails in `mozangle`'s build script with a bindgen panic.
- On this workstation, LLVM 23.1.2 is at `G:\Git\better-web-browser\target\webgl2-admission-20261004\llvm-paired\clang+llvm-23.1.2-x86_64-pc-windows-msvc\bin`; set `LIBCLANG_PATH` to it. Do not delete that directory when cleaning `target`. CMake comes from Visual Studio 2022 Build Tools, which `scripts/prepare-build-tools.ps1` finds through `vswhere`.
- Run repository scripts with PowerShell 7 (`pwsh`, installed through winget as `Microsoft.PowerShell` and reached through the `WindowsApps` alias on PATH). The scripts rely on `$IsWindows` and .NET Core APIs, and the full-test runner fails under Windows PowerShell 5.1, whose redirection turns native stderr into terminating errors.
- Prepare a build shell in `pwsh` with: `$env:LIBCLANG_PATH = '<LLVM bin above>'; $env:CARGO_TARGET_DIR = 'G:\Git\better-web-browser\target'; . ./scripts/prepare-angle.ps1; . ./scripts/prepare-build-tools.ps1`, then run Cargo with `-j 4`. The final gate is `./scripts/run-impacted-tests.ps1 -Final -Run` from that shell.

## Shared-machine resources and performance work

- This workstation also runs other agents. Default local Cargo builds to four jobs, and run heavy builds and full suites sequentially unless the user requests otherwise. The complete local verification runner may use three isolated, single-thread CPU test processes and up to two hidden browser workers for its explicitly classified independent functional fixtures, as authorized by the user. Keep heavy native GPU tests to one worker; run memory, latency and performance comparisons exclusively after the CPU and functional browser workers finish. Unknown responsibilities stay serial. Do not terminate another agent's processes or the user's browsers to obtain cleaner measurements.
- During development, run focused tests for the changed responsibilities instead of repeatedly running unrelated suites. Run the full local verification gate once the implementation is ready for a PR; use focused tests to repair any failure before repeating that final gate.
- Use `scripts/run-impacted-tests.ps1` to inspect responsibility-based test selection during iteration. Keep its ownership/dependent-consumer map and isolated contract tests current as modules split; shared coordinators and unknown changes must select broader coverage, never a guessed safe subset. This local selector does not reduce the final pre-PR verification gate or change CI.
- Optimize measured slow commands with small reproducible workloads, not a live application's end-to-end loading clock. Validate actual results/pixels and preserve ordering, ownership and error behavior. Keep the unchanged application as a separate functionality check.
- Run timing comparisons without this agent's builds or heavy suites overlapping; rotate sequential candidate/reference order and retain individual samples and ranges. Other agents still affect scheduling, so do not silently discard busy-machine outliers or present elapsed time as isolated CPU cost.

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
- Before handing any pull request to the user for review, run `./scripts/prepare-v8.ps1 -Profile release`, rebuild the local release browser from the final PR head with `cargo build --release --locked --bin better-web-browser`, and report the exact executable path. If the branch changes after that build, rebuild it again before handoff.
- Push the topic branch, open a pull request, and wait for all required checks. Only the user may merge pull requests; never run `gh pr merge` or enable auto-merge. Stop at "ready to merge" and provide the pull-request link.
- Write each PR-body prose paragraph and list item on one physical line; let GitHub wrap it responsively. Do not hard-wrap prose at a source-file column limit or use wide fenced blocks for reports. Keep before/after tables compact, and inspect the submitted body for manual line breaks before handoff.
- In this Windows workspace, an escalated `gh pr create` can fail when its internal Git process sees the repository as owned by the sandbox identity. Create PRs with the fully qualified GitHub API endpoint (`gh api repos/VictorZakharov/better-web-browser/pulls --method POST ...`) so no local repository inference is needed; do not mutate the user's global `safe.directory` configuration as a workaround.
- Merge pull requests with GitHub's merge-commit method only. Squash merging and rebase merging are disabled for this repository.
- Preserve the required check names `windows` and `Linear PR history` when editing CI. The `windows` gate aggregates the parallel source/format, browser-smoke, dependency-policy and harness workers; coordinate any intentional rename with branch protection.
- CI is a smoke gate, not full standards compliance: both source PRs and pushes to `main` run the dedicated small browser smoke target, source/format, dependency policy, and harness self-tests. Run full-target Clippy and relevant full unit, integration, WPT/Khronos, and visual suites locally before push. Markdown-only pull requests may skip Windows workers only when the checked-in classifier confirms every changed path ends in `.md`; main never skips its smoke workers. Keep classification fail-safe and make `windows` reject inconsistent classifier/worker results.
- Optimize CI for wall-clock feedback. Independent checks may spend additional standard hosted-runner minutes in parallel; do not serialize them merely to minimize total runner usage. Preserve the compiler-level sccache configuration and Cargo registry caches, and measure end-to-end timing after material CI changes. Do not cache `target` directly: checkout timestamps and V8's large prebuilt archive make whole-target caches ineffective.
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
