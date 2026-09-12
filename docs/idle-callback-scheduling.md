# Cooperative idle scheduling

Implemented for the retained Window runtime, September 2026; tracks
[#94](https://github.com/VictorZakharov/better-web-browser/issues/94).
The contract follows [Cooperative Scheduling of Background Tasks](https://w3c.github.io/requestidlecallback/)
and [Web IDL conversions](https://webidl.spec.whatwg.org/#es-unsigned-long).

## What changed

| Behavior | Previous implementation | Scheduler-backed implementation |
| --- | --- | --- |
| Idle opportunity | Ordinary timer with a fixed 50 ms delay | Separate pending/runnable queues, selected after ready ordinary work |
| Deadline | Author-visible properties and `performance.now()` | Private deadline state and native monotonic elapsed time, capped at 50 ms and upcoming scheduled work |
| Timeout | `didTimeout` chosen when registered | Timeout races idle execution; an expired callback runs once with `didTimeout=true` and zero remaining time |
| Cancellation | Alias of `clearTimeout` | Document-owned idle handles; removes pending, runnable, and timeout work without cancelling ordinary timers |
| Reposting | Another ordinary delayed timer | Only eligible in a later idle period, after older runnable callbacks |
| Busy tasks | Renderer time could disappear at acknowledgement | Apply elapsed time before selecting tasks; retain the shell's dispatch-time clock across replies |

Each idle callback and its microtask checkpoint return control to the embedder. Accepted native
input, fetch/worker completions, media and document lifecycle tasks, and rendering-observer work interrupt
the current idle period. Ready dynamic scripts and unsubmitted fetch/worker work prevent starting
an idle period. Expired timeout callbacks remain eligible under load. Callback exceptions reach
the global error event. Document cancellation destroys the realm and its queued idle work.

`IdleDeadline` has private, read-only timeout state and receiver-checked accessors. Remaining time
never uses a page-overridden clock and is rounded down to millisecond precision, matching the
current Performance clock's resolution. Timeout conversion uses Web IDL `unsigned long`; zero
does not request timeout delivery. A timeout registered partway through a callback starts at that
registration point, rather than at the callback's beginning.

## Verification

The curated gate adds 12 unchanged upstream files (22 assertions) from the existing pinned WPT
checkout. They exercise deadlines, busy-loop timeouts, cancellation, exception reporting,
ordering, and reposts during animation. The complete curated gate is now 127 files / 735
assertions, with no expected failures. No upstream test source is copied into this repository.

Seventeen focused native/runtime tests additionally cover independent handle namespaces, private
deadline state, Web IDL argument conversion, input/render interruption, expired deadlines,
registration time, cancellation during dispatch, and old-document cleanup. MediaSource event
tasks interrupt idle periods and finish their microtask checkpoint before idle work resumes.

The original `tests/fixtures/idle-workload.html` mixes bounded background work with a fetch,
timers, animation frames, and a button. Hidden Breeze and unified-headless, muted Chrome both
completed 20 idle callbacks, 20 timers, 20 animation frames, the fetch, and one injected native
click, with no fixture failures. Screenshots were inspected for the final counters. This is a
functional scheduling check, **not a page-loading speedup or visual-fidelity claim**.

Reproduce the upstream checks with:

```powershell
./scripts/checkout-wpt.ps1 -Destination ../wpt
./scripts/run-wpt.ps1 -WptRoot ../wpt -Filter 'Idle callbacks'
cargo test --lib idle_
```

For the owned page, serve `tests/fixtures` through `scripts/serve-alpha-fixtures.ps1`, then run
`scripts/run-hidden-benchmark.ps1` against `/idle-workload.html` with `-SettleMs 2500`,
`-ClickTarget '100,100' -NavigationDelayMs 100 -FreshProfile` and a screenshot output. Use the
checked-in Chromium baseline harness with `--settle-ms 2500 --click-after-ready 100,100
--navigation-delay-ms 100`. Captures/reports belong in ignored `target` output, not commits.

## Boundaries

Idle work is cooperative: a callback that ignores its deadline cannot be preempted at 50 ms.
The existing script watchdog remains the safety boundary. Idle work has no promised start latency
without a positive timeout. This implementation uses Breeze's current 16 ms animation scheduling;
it does not introduce hardware-vsync scheduling or claim browser-wide event-loop conformance.
The API remains Window-only. Cross-origin iframe scheduling, suspended-frame behavior, and
cross-realm WPT cases are outside this focused gate; broader frame lifecycle work remains separate.
No YouTube or MSN special cases, security-policy changes, or new dependencies are involved.
