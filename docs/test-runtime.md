# Complete local test runtime

The goal is a complete warm local verification gate below five minutes. Measure full-target Clippy, test compilation, every discovered test harness, doc tests and runner contracts together. Hosted CI and the final release executable are separate clocks; a faster local test run does not establish a faster release build or hosted gate. [Issue #245](https://github.com/VictorZakharov/better-web-browser/issues/245) tracks the broader feedback time.

## Measured result: 2026-10-15

| Scope | Before | After |
| --- | ---: | ---: |
| Complete local gate | 15m 37s | 5m 01.70s |
| Tests, doc tests and runner contracts, excluding Clippy/compilation | 14m 00.33s | 4m 59.26s |
| Active Rust tests passed | 6,723 | 6,728 |
| Ignored tests | 25 | 26 |
| Same-binary twelve-fixture workload, one/two workers | 6.69–7.08s | 3.47–3.87s |

The complete gate is about 68% shorter, but remains 1.70s above five minutes. The final sample reused compiled artifacts: Clippy took 0.76s and test compilation 0.91s. The earlier core-source edit cost 22.43s in Clippy and 25.78s compiling tests; core edits and cold builds therefore still exceed the complete five-minute budget. The execution-only figures subtract the recorded Clippy and compilation stages from each complete observation; they include scheduling and contracts. These shared-workstation observations use different source trees and are not a controlled same-edit compiler benchmark.

The final complete run retained all 6,754 discovered test entries, including 26 ignored entries. The additional ignored entry is the subprocess CPU-guard probe executed by an active parent test. Its report is `target/test-reports/20261015T191028651Z/full-tests.json`; the tested candidate was an owned dirty tree on base `00dcd67b`, with no further executable-source edits before committing it. Incremental compiler directories occupied 4.31 GiB after the experiments on G:; that is a snapshot, not a measured delta from a clean baseline.

## Running and evidence

Prepare LLVM/CMake and debug V8 as described in [PR feedback time](pr-feedback-time.md), then run `./scripts/run-impacted-tests.ps1 -Final -Run` as the normal Windows user. It restores the caller's environment and defaults to four build jobs, zero debug information and incremental compilation on G:. Explicit debug-info settings and `CARGO_INCREMENTAL=0` override those defaults. The first incremental build populates additional compiler state; warm rebuilds and its disk cost must be reported separately.

`scripts/run-full-tests.ps1` compiles all targets once and discovers tests from Cargo's actual test executables, including binary and example harnesses. It runs the complete set, verifies each worker's exact discovered names and passed/ignored counts, and rejects nonzero exits, missing or duplicate tests and unknown artifact kinds. Existing ignored tests remain ignored; the additional ignored CPU-guard probe is executed by an active parent test. Doc tests and PowerShell ownership/process contracts also run. This is a scheduler for the complete gate; responsibility-based development subsets remain in `run-impacted-tests.ps1`.

Each complete run retains `full-tests.json`, fresh library/browser plans, compilation output and per-worker stdout/stderr under `target/test-reports/<UTC timestamp>/`, including failure evidence. The outer gate also includes Clippy; include its wall time when reporting the complete command. Reports identify the Git revision; record a dirty-tree status when comparing uncommitted candidates. A worker's elapsed time includes scheduling and output observation on this shared workstation.

## Resource ownership

- Three isolated CPU library processes each use one libtest thread. Only explicitly classified responsibilities enter them; mixed parents, ambiguous substring filters and unknown modules remain native. Test-only guards exit the owned process before the library's ANGLE/D3D factories initialize when CPU classification is wrong. A failed first probe caught a canvas-presentation module; that module now remains native. Relative scheduling weights distinguish cheap parsing/layout tests from V8, pixel, codec and real-deadline tests; weights affect balance, never coverage.
- One native worker runs GPU library tests and ordinary integration harnesses. Known native renderer functionality also runs serially alongside guarded CPU work, before the two functional renderer workers; it never overlaps another GPU worker. Socket throughput comparisons run exclusively after CPU workers finish.
- At most two hidden workers run explicitly listed independent renderer-process functional fixtures, followed later by the independent full-browser fixtures. The renderer phase waits for enough CPU workers to finish to cap active test processes at four; renderer-exclusive cases then wait for all CPU workers. Browser fixtures use separate temporary profiles and ephemeral server ports. This concurrency was explicitly authorized by the user.
- Unknown browser responsibilities, streaming peak-memory checks, wheel latency/cadence and fullscreen fixtures run exclusively. Heavy GPU cases use one serial native worker. Do not extend the concurrent allowlist without checking ownership and measurement requirements.
- All test processes use `CreateNoWindow`. Breeze child launches retain `--benchmark` and `BREEZE_REQUIRE_HIDDEN_BENCHMARK`; renderer children retain their existing `CREATE_NO_WINDOW` launch path. Failure cleanup stops only process handles owned by this invocation.

## Removing idle tails

Finite browser fixtures signal completion only after their asserted terminal operations. Existing console markers or `--completion-title` replace idle settle periods while retaining outer failure deadlines, exact results, payload sizes, native pixels and ordering assertions. The exact title condition is benchmark-only, bounded to the diagnostic title size and mutually exclusive with a console completion marker. It observes an accepted presentation, so an early console message or title prefix cannot finish a later painted result. Timing/endurance fixtures retain their observation windows.

Keepalive fixture servers observe the full browser lifetime, drain queued connections and join all accepted request handlers after the browser exits. This replaces the server's fixed five-second idle tail while retaining its failure deadline and all positive/negative request and cookie assertions.

One worker diagnostic-budget test previously consumed six real ten-second watchdog failures. It still exercises six actual infinite-callback terminations, with a per-agent, test-only 100ms deadline around just those callbacks. Normal tasks and explicit production-deadline tests retain the ten-second budget. The scope and original timeout are checked independently; production code has no timeout override.

The wheel backlog fixture now blocks the final forward input before injecting the reverse input. This establishes the intended active-animation precondition before its 250ms delayed acknowledgment; the baseline sometimes injected reversal after that delay expired. Assertions about physical positions, ordering, trusted input, cancellation and subsequent frames remain intact. Keep that failed baseline in the timing evidence.

## Baseline and experiments

The preceding complete gate took 15m 37s, with 6,723 passed and 25 ignored. Separate per-test baseline probes observed 400.18s for the library (5,817 passed, 14 ignored) and 348.47s for live runtime (137 passed, one wheel-fixture setup failure, three ignored). These are separate shared-workstation observations, not a summed full-gate measurement.

Lightly optimizing the entire library test binary cost 211.72s to compile and gave only a small improvement for the sampled canvas text-shadow tests; it was not adopted. Local compiler query reuse and explicit scheduling preserve the existing correctness profiles instead. Test fixtures still compare every pixel byte; text-shadow assertion diagnostics are constructed only on mismatch.

The first complete candidate passed all 6,728 active tests in 6m 00s, with 26 ignored. A subsequent core-source edit took 22.43s in Clippy and 25.78s in test compilation; all Rust tests passed, but the final gate rejected an incorrect synthetic planner expectation. That expectation was repaired. Keep this failed gate and its compilation cost in the record; it is not a successful under-five-minute result.

Controlled worker samples used the identical executable and twelve finite parser/title fixtures, with no overlapping build or suite from this agent. Each sample retained exact results and painted output. Order was two workers, one, one, two; individual elapsed times were 3.87s, 7.08s, 6.69s and 3.47s. The observed ranges were 6.69–7.08s serial and 3.47–3.87s with two workers. This demonstrates the scheduling benefit on that workload, not a universal twofold speedup.

Other focused evidence retained on G: includes all five keepalive cases passing in 11.03s (22.17s in the baseline probe) and six module/script/upload cases passing in 5.45s. The wheel baseline failure, an initial fail-closed CPU/GPU classification probe, and two runner startup repairs (example harnesses and empty terse discovery) remain part of the investigation record. No failed samples were silently discarded.
