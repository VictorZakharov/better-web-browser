# ADR 0006: Select V8 for the production JavaScript engine

- Status: Accepted and implemented
- Date: 2026-08-28
- Issue: [#113](https://github.com/VictorZakharov/better-web-browser/issues/113)

## Context

Breeze embedded a locally patched Boa 0.21.1 interpreter. A controlled hidden comparison on live
MSN measured 12,878.5 ms of Boa JavaScript execution against 871.8 ms in Chromium, a 14.77x gap.
Networking, parsing, the native DOM bridge, style, layout, and presentation were not the dominant
costs.

Issue #113 originally proposed reducing that gap to 5x Chromium. The product requirement is now
stricter: median JavaScript time and populated-content readiness must be no more than 1.10x
Chromium under the same controlled conditions. Site-specific script skipping is not acceptable.

Boa remains useful as a pure-Rust embeddable interpreter, but it is not a credible route to this
target:

- Boa describes itself as experimental and does not ship a JIT.
- Its 2026 roadmap estimates 175-350 hours for VM/runtime performance work and 350 hours for a
  production-ready garbage collector.
- The public Cranelift prototype reports mixed 0.86x-2.16x changes on the V8 benchmark suite. It
  also identifies property access, calls, allocation, and garbage collection as dominant costs, so
  bytecode dispatch compilation alone does not close the gap.

Primary sources:

- [Boa project](https://github.com/boa-dev/boa)
- [Boa 2026 roadmap](https://boajs.dev/roadmap)
- [Boa JIT discussion and prototype results](https://github.com/boa-dev/boa/discussions/4487)
- [V8 execution tiers](https://v8.dev/blog/maglev)
- [Rusty V8](https://github.com/denoland/rusty_v8)
- [Servo's Rust SpiderMonkey bindings](https://github.com/servo/mozjs)

## Decision evidence

Before the production migration, a contained release-mode probe executed one repository-owned,
production-shaped workload through Boa, V8-jitless, and V8-JIT in separate real renderer processes.
The workload performed keyed tree reconciliation using functions, objects, arrays, maps, strings,
property reads/writes, and allocation churn. Every engine returned the same checksum.

The JIT sample removed only Windows' dynamic-code prohibition. It still:

- started in the capability-free AppContainer with no console and the minimal environment;
- could not create child processes;
- could not reach browser-owned loopback; and
- could not reach the Internet directly.

Six optimized samples across two independent invocations on the same Windows machine produced:

| Metric | Boa | V8-jitless | V8-JIT |
| --- | ---: | ---: | ---: |
| Total median | 480,569 us | 57,911 us | 41,860 us |
| Evaluation median | 479,676 us | 51,857 us | 34,767 us |
| Setup median | 868 us | 3,525 us | 3,936 us |
| Boa-relative evaluation | 1.00x | 9.25x faster | 13.80x faster |

V8-JIT was 1.49x faster than V8-jitless for evaluation. The 13.80x owned-workload result is close
to the independently observed 14.77x live-MSN deficit. This demonstrates that the engine is the
dominant performance constraint; it does not by itself prove that the completed Web bindings meet
#113. Live MSN must be remeasured after the production switch.

The temporary dual-engine executable measured 62,790,144 bytes versus 19,253,248 bytes for the Boa
executable. The final single-engine V8 release executable is 53,778,944 bytes (51.29 MiB), 8.59 MiB
smaller than the dual-engine probe and 32.93 MiB larger than the former Boa executable.

The same-machine hidden live-MSN comparison was repeated after the production switch with the same
15-second settle period used for the original measurement. Three fresh-profile samples produced:

| Metric | Boa before migration | V8 after migration | Chromium reference |
| --- | ---: | ---: | ---: |
| Median JavaScript time | 12,878.5 ms | 5,214.5 ms | 706.8 ms |
| Breeze/Chromium ratio | 14.77x | 7.38x | 1.00x |

V8 reduces Breeze's measured JavaScript time by 59.5% and halves the relative gap. All three V8
samples returned HTTP 200 without a JavaScript error, runtime stop, or renderer exit. However, the
7.38x result does not meet #113's 1.10x requirement. At 15 seconds MSN executes 92-93 scripts,
performs about 20,000 DOM mutations, and triggers 33-34 Breeze render checkpoints. The engine
migration therefore removes the interpreter as the primary blocker and exposes the remaining host
binding, mutation invalidation, and rendering work. The benchmark's current first-layout
`page_ready_ms` is not a populated-content metric, so it cannot be used to claim the second #113
acceptance criterion either.

## Decision

Migrate Breeze's production JavaScript engine from Boa to V8 through the exact locked `v8` Rust
binding. V8 is the sole production engine; Breeze will not ship a Boa fallback, user-selectable
dual-engine mode, or site-specific execution bypass.

Keep Web API and browser policy in Breeze. The engine boundary owns realm/value conversion,
evaluation, exceptions, modules, promises and microtasks, interruption, and V8 lifetime. Breeze
continues to own DOM, Fetch, storage, navigation, timers, task ordering, document generations, and
the browser/renderer IPC contracts.

Permit dynamic code only inside the already-contained AppContainer renderer so V8's JIT can run.
Retain DEP, SEHOP, forced relocation, heap-corruption termination, bottom-up and high-entropy ASLR,
strict handle checks, extension-point disabling, CFG, remote/low-integrity image blocking, the Job
Object, child-process denial, minimal environment, network denial, bounded IPC, watchdogs, and
memory limits.

Pin V8 versions and treat archive provenance and Chrome Stable security updates as release
infrastructure. An overdue applicable V8 security update is a release blocker. The JIT permission
does not extend to the browser process.

V8 is preferred over SpiderMonkey because Chromium is the performance reference, rusty_v8 tracks
Chrome's release cadence, and using the same engine removes a major source of benchmark variance.
SpiderMonkey remains a contingency if V8 cannot meet containment or build requirements. QuickJS is
not selected because another interpreter migration does not address the demonstrated requirement
for production-tier compilation.

## Implementation status

The production code and locked dependency graph use V8 152.2.0 exclusively. Engine-specific
code lives behind focused adapters in `src/engine/script/engine/`; Web API hosts exchange owned,
engine-neutral values across the `__hostCall` boundary. Document and Worker realms, retained
functions, typed arrays, static module graphs, `import.meta.url`, top-level await, and Promise
completion use V8 directly. A per-isolate watchdog terminates a JavaScript entry after two seconds
in production and 100 ms in unit tests.

The Boa dependencies, local `vendor/boa_*` forks, temporary probe feature and commands, and the
probe-only scripts have been removed. `scripts/prepare-v8.ps1` stages the official locked V8 archive
or reuses a verified local artifact, checks both the archive and decompressed-library SHA-256, and
creates a non-privileged NTFS junction only when Cargo's registry and target directories are on
different drives. `.cargo/config.toml` also pins the archive checksum. CI, release, WPT, and local
release instructions invoke preparation before native V8 builds.

The AppContainer renderer now permits V8's JIT while retaining the other mitigations and capability
denials listed above. The complete renderer-process suite verifies that the production renderer
still cannot create a child process, reach browser-owned loopback, or reach the Internet directly.
At this revision 400 library tests, 30 renderer-process tests, 16 live-runtime tests, 610 curated WPT
subtests, five hostile-input tests, and four HTML parser conformance tests pass. The locked release
browser builds successfully from the production V8-only dependency graph. Third-party notices were
regenerated from that graph, and the hidden live-MSN evidence above completes the migration checks.

## Consequences

- Release builds acquire V8's native archive and take longer to compile and link.
- The JavaScript engine must follow Chrome's security-update cadence.
- The V8 callback boundary uses unsafe FFI internally and therefore receives focused lifecycle,
  multi-realm, exception, termination, module, Worker, and containment tests.
- Browser-authoritative Fetch and persistent-state policy does not move into V8.
- Meeting #113 remains a measured acceptance criterion; engine replacement alone is not a claim
  that MSN compatibility or the 1.10x target is complete.
