# ADR 0006: Select V8 for the production JavaScript engine

- Status: Accepted for migration
- Date: 2026-08-28
- Issue: [#113](https://github.com/VictorZakharov/better-web-browser/issues/113)

## Context

Breeze currently embeds a locally patched Boa 0.21.1 interpreter. A controlled hidden comparison
on live MSN measured 12,878.5 ms of Boa JavaScript execution against 871.8 ms in Chromium, a
14.77x gap. Networking, parsing, the native DOM bridge, style, layout, and presentation were not the
dominant costs.

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

## Contained release probe

The feature-gated v8-engine-spike path executes one repository-owned, production-shaped workload
through Boa, V8-jitless, and V8-JIT in separate real renderer processes. The workload performs keyed
tree reconciliation using functions, objects, arrays, maps, strings, property reads/writes, and
allocation churn. Every engine must return the same checksum.

The normal renderer launch prohibits dynamic code. The V8-JIT sample removes only that mitigation;
the default remains unchanged. The test proves that the JIT sample still:

- starts in the capability-free AppContainer with no console and the minimal environment;
- cannot create child processes;
- cannot reach browser-owned loopback; and
- cannot reach the Internet directly.

renderer_mitigations has a regression test proving that the spike policy changes only the
dynamic-code bit. DEP, SEHOP, forced relocation, heap-corruption termination, bottom-up and
high-entropy ASLR, strict handle checks, extension-point disabling, CFG, and remote/low-integrity
image blocking remain enabled.

Six optimized samples across two independent invocations on the same Windows machine produced:

| Metric | Boa | V8-jitless | V8-JIT |
| --- | ---: | ---: | ---: |
| Total median | 480,569 us | 57,911 us | 41,860 us |
| Evaluation median | 479,676 us | 51,857 us | 34,767 us |
| Setup median | 868 us | 3,525 us | 3,936 us |
| Boa-relative evaluation | 1.00x | 9.25x faster | 13.80x faster |

V8-JIT was 1.49x faster than V8-jitless for evaluation. The 13.80x owned-workload result is close
to the independently observed 14.77x live-MSN deficit. This is evidence that the engine is the
dominant performance constraint, not proof that the completed V8 Web bindings already satisfy
#113. Live MSN must be remeasured after migration.

The first feature-enabled optimized build in the existing worktree took 4 minutes 54 seconds. The
temporary dual-engine executable measured 62,790,144 bytes versus 19,253,248 bytes for the default
Boa executable: a 43,536,896-byte (41.52 MiB) increase and a conservative upper bound because the
completed migration removes Boa. Final packaging size must be remeasured after that removal.

Rusty V8's Windows build also tries to create a privileged directory symlink when Cargo's registry
and target are on different drives. `scripts/prepare-v8-engine-spike.ps1` creates a checked,
non-privileged NTFS junction for debug or release builds, and `scripts/run-v8-engine-spike.ps1`
invokes it before running the hidden contained samples.

## Decision

Migrate Breeze's production JavaScript engine from Boa to V8 through the pinned v8 Rust bindings.

During migration:

1. Treat Boa as temporary migration scaffolding, not a supported fallback backend. Switch the
   production default as soon as V8 passes the existing runtime, renderer-process, live, and
   curated WPT suites, then delete Boa.
2. Move engine-specific realm, value, evaluation, exception, module, promise/microtask, interrupt,
   and memory-limit operations behind focused adapters. Do not abstract DOM/layout/network policy
   into the JavaScript engine.
3. Reuse the existing browser-owned __hostCall policy boundary concept with V8 callbacks; keep
   Fetch, storage, navigation, timers, task ordering, and document generation owned by Breeze.
4. Permit dynamic code in the AppContainer renderer when V8 becomes the production backend.
   Retain all other current launch mitigations and the Job, handle, process, environment, network,
   IPC, timeout, and memory controls.
5. Treat V8 archive provenance as release infrastructure: pin versions, verify upstream checksums
   or attestations, cache through a controlled mirror, and retain the ability to build from source.
6. Track Chrome Stable security releases and treat an overdue applicable V8 security update as a
   release blocker. JIT permission is limited to the already contained renderer process.
7. Measure live MSN on every migration stage. The switch is complete only when #113's 1.10x
   JavaScript and populated-readiness thresholds pass.

The migration is incomplete until the Boa dependencies and local `vendor/boa_*` forks are removed,
the temporary `v8-engine-spike` feature is gone, and the default locked release graph contains V8
without Boa. Breeze will not ship a user-selectable dual-engine mode.

V8 is preferred over SpiderMonkey because Chromium is the performance reference, rusty_v8 tracks
Chrome's release cadence, and using the same engine removes a major source of benchmark variance.
SpiderMonkey remains the fallback if V8 cannot meet Breeze's containment or build requirements.
QuickJS is not selected because another interpreter migration does not address the demonstrated
requirement for production-tier compilation.

## Scope and estimate

Boa types currently touch 13 Breeze source files containing about 2,708 lines. The DOM, CSS, layout,
network broker, renderer protocol, and browser shell do not need replacement, but engine values,
rooting, modules, promises, workers, and interruption do.

The provisional migration estimate is 48-80 active implementation and verification hours:

- platform, isolate/realm, value, callback, exception, and limit adapters: 12-20 hours;
- document bootstrap, tasks/microtasks, timers, modules, and Fetch settlement: 16-24 hours;
- workers, structured clone, storage, and lifecycle parity: 12-20 hours; and
- conformance, live benchmarks, dependency removal, packaging, and CI optimization: 8-16 hours.

Split these into reviewable PRs and revise the remaining estimate after the first production adapter
lands. Do not attempt a one-PR engine replacement.
