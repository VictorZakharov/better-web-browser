# ADR 0005: Boa runtime Web API evaluation

- Status: Superseded by [ADR 0006](0006-v8-engine-selection.md)
- Date: 2026-08-23
- Issues: [#55](https://github.com/VictorZakharov/better-web-browser/issues/55),
  [#47](https://github.com/VictorZakharov/better-web-browser/issues/47)

## Context

Breeze embeds a locally patched `boa_engine` 0.21.1 and supplies browser APIs around the engine.
Before adding more handwritten bindings, issue #55 required a time-boxed review of the matching
official [`boa_runtime` 0.21.1](https://docs.rs/boa_runtime/0.21.1/boa_runtime/) crate. The question
was whether its extensions could delete Breeze code without weakening the browser-owned Fetch
broker, document lifecycle, origin policy, resource budgets, or event-loop ordering.

The evaluated source is tag `v0.21.1`, upstream commit
[`bc36c3f`](https://github.com/boa-dev/boa/tree/bc36c3fac0969ea21ea0570b62e7846f97389b73/core/runtime).
The crate declares `Unlicense OR MIT`, matching the already-vendored Boa engine, and requires Rust
1.88. Breeze's current toolchain is new enough. The upstream package describes itself as an example
runtime for runtime implementors, so its Web-shaped APIs are integration candidates rather than a
browser conformance boundary.

## Decision

Do **not** add a production or development dependency on `boa_runtime` 0.21.1. Keep the existing
Breeze implementations and the engine primitives they already use. Re-evaluate a later runtime
release when one or more candidates can replace policy-bearing code and pass the same regression
and WPT gates.

`BlockingReqwestFetcher` is rejected unconditionally. Enabling it would create renderer-owned
network authority alongside the typed browser-process Fetch broker.

### Candidate matrix

| Candidate | Decision for 0.21.1 | Reason |
| --- | --- | --- |
| `fetch`, `Headers`, `Request`, `Response` | Reject | The upstream `RequestInit` carries only method, headers, and a string body. A request containing `Host` and `Cookie` constructed successfully, and the source lowers both without a request-header guard; request mode, credentials, signal, and even the public request headers property were absent. Response body state was also absent. The `Fetcher` hook receives this already-lowered request and has no contract for CORS, redirects, credentials, cancellation, document identity, filtered responses, or byte budgets. Adapting it would duplicate rather than replace ADR 0004. |
| `URL` and `URLSearchParams` | Defer | `URL.searchParams` explicitly throws and no `URLSearchParams` global is registered. Breeze needs the live URL/query coupling and currently passes all five pinned URL cases. The upstream `url`-crate-backed setters remain worth reconsidering after the pair is complete. |
| `TextEncoder` and `TextDecoder` | Reject | The encoder accepts non-standard UTF-16 modes, emits UTF-16 bytes, and lacks `encodeInto`. Decoder options are accepted but the `fatal` and `ignoreBOM` behavior/properties and streaming contract are absent. Breeze's UTF-8 implementation covers these contracts and replacement would be a regression. |
| `queueMicrotask` | Adapt without the crate | Its use of Boa's job queue confirms Breeze's existing engine-level approach. In the probe, a throwing callback made `run_jobs` fail and prevented the next queued microtask from running. Breeze must retain its wrapper because it reports the exception through a cancelable global `ErrorEvent`, continues the checkpoint, and performs checkpoints between document-scoped task sources. |
| `structuredClone` and `JsValueStore` | Defer | Transfer detachment worked in the probe, and the sendable store is a strong future reuse candidate. It does not provide Breeze's hooks for browser-defined `Blob`/`File` values or its current serialized document/worker transport. Adopt only when host-type hooks let it replace both the public clone and messaging serializers. |
| `postMessage` | Defer | `MessageSender` and its stop hook are useful, but the extension delegates target-origin interpretation and delivery completely to the host. Breeze would still own document generation, source/origin, trusted `MessageEvent` construction, task queuing, host values, and cancellation, leaving two serialization paths. |
| Console support | Defer | A custom `Logger` could route output to Breeze, but the current synchronous bridge already owns renderer capture and diagnostic formatting. This small replacement alone does not justify the dependency and engine-feature cost. |
| Timers | Reject | Runtime timers enqueue generic `TimeoutJob`s in the context. They have no document-generation token, browser task-source ownership, stale-document cancellation, per-task mutation budget, or Breeze's deterministic hidden-test clock. They cannot replace the document scheduler. |

## Compile and integration probe

The probe used disposable crates under the ignored `target` directory; no probe source, lockfile, or
dependency remains in the repository. Both variants used the vendored `boa_engine` and Rust 1.95.0.
The runtime variant pinned `boa_runtime = "=0.21.1"`, kept its default `fetch` and `url` features,
and did not enable `reqwest-blocking`.

Measurements are developer-machine diagnostics, not release-performance claims. Each clean check
used a separate empty target directory; each cached check immediately repeated the same command.

| Measurement | Engine baseline | With runtime | Delta |
| --- | ---: | ---: | ---: |
| Clean `cargo check` | 22.185 s | 23.799 s | +1.614 s (+7.3%) |
| Cached `cargo check` | 0.302 s | 0.339 s | +0.037 s |
| Check target size | 380,740,847 B | 412,139,363 B | +31,398,516 B (29.9 MiB) |
| Debug context setup, 200-sample mean | 1,978 us | 2,265 us | +287 us (+14.5%) |

The registration timing covered encoding, microtask, structured clone, URL, and Fetch extensions.
It measures context construction plus registration, not page runtime after startup.

A paired metadata resolution of Breeze's dependency manifest showed that the runtime's default
features introduced 14 packages. Disabling the runtime's default features still introduced 12. All
declared licenses were permissive (`MIT`, `Apache-2.0`, or Boa's `Unlicense OR MIT`), so provenance
and licensing are not blockers. The base cost includes futures support and activates `boa_engine`'s
default `float16` and `xsum` features through Cargo feature unification even though Breeze's direct
engine dependency disables default features. The Fetch feature added `bytes` and `http`; its other
dependencies were already present in Breeze's graph.

The semantic probe produced these compatibility observations:

| Observation | Result |
| --- | --- |
| Basic microtask ordering | `sync`, then queued microtask, then promise reaction |
| Throwing microtask followed by another microtask | `run_jobs` failed; the later microtask did not run |
| `URLSearchParams` global | Absent |
| `URL.searchParams` | Throws an explicit not-implemented error |
| `TextEncoder.prototype.encodeInto` | Absent |
| `new TextEncoder('utf-16le').encode('A')` | `65,0` instead of the Web Encoding UTF-8 contract |
| `TextDecoder(..., { fatal: true }).fatal` | Absent |
| Structured-clone transfer | Source detached; four-byte clone retained |
| `Host` and `Cookie` in `RequestInit` | Request constructed; source lowers both without a request-header guard |
| Request mode, credentials, signal, and headers properties | Absent |
| Response `bodyUsed` | Absent |

These results agree with the version-pinned upstream
[`fetch`](https://github.com/boa-dev/boa/blob/bc36c3fac0969ea21ea0570b62e7846f97389b73/core/runtime/src/fetch/mod.rs),
[`request`](https://github.com/boa-dev/boa/blob/bc36c3fac0969ea21ea0570b62e7846f97389b73/core/runtime/src/fetch/request.rs),
[`URL`](https://github.com/boa-dev/boa/blob/bc36c3fac0969ea21ea0570b62e7846f97389b73/core/runtime/src/url.rs),
[`text`](https://github.com/boa-dev/boa/blob/bc36c3fac0969ea21ea0570b62e7846f97389b73/core/runtime/src/text/mod.rs), and
[`interval`](https://github.com/boa-dev/boa/blob/bc36c3fac0969ea21ea0570b62e7846f97389b73/core/runtime/src/interval.rs)
implementations.

## Conformance comparison

The matching Breeze surfaces were run without visible browser UI against WPT revision
`f9ecd8a4a9c6e9865ea4aee4741e4b02f75fd476`:

- Fetch API: 23 pass, 0 regression;
- URL: 5 pass, 0 regression; and
- event-loop microtasks: 3 pass, 0 regression.

Ten focused Rust regressions also passed: the six script compatibility tests plus encoding/window
messaging, structured-clone transfer, timer/microtask checkpoints, and global microtask exception
reporting. Because no runtime candidate was adopted, no new compatibility fork or parallel test
contract was added.

## Effect on issue #47

Issue #47 should continue on Breeze's existing typed implementation:

- keep `Headers`, `Request`, and `Response` policy in the renderer Web bindings;
- translate only validated, bounded Fetch intents through ADR 0004;
- settle completion at document-owned task and microtask checkpoints;
- keep module loading on the current Boa module loader plus brokered resource path; and
- use the existing navigation/document-generation cancellation checks.

The evaluation does not add an adapter phase or change #47's estimate. Any remaining work is a gap
audit against its acceptance criteria, not a refactor through `boa_runtime`.

## Re-evaluation triggers

Review a newer matching Boa release when at least one of these is true:

- Fetch exposes complete Web request state plus host hooks for CORS, redirects, credentials,
  filtering, cancellation, and budgets before I/O;
- URL includes a live, conforming `URLSearchParams` implementation;
- Encoding passes Breeze's `encodeInto`, decoder option, streaming, and error cases;
- structured clone exposes browser host-type hooks usable by both window and worker transport;
- timer and messaging extensions accept document-generation/task-source integration; or
- several safe candidates together justify the dependency graph and engine-feature activation.

Until then, `Cargo.toml`, `Cargo.lock`, and `THIRD_PARTY_NOTICES.md` intentionally remain unchanged.
