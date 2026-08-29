# Architecture decisions

Architecture decision records document boundaries that are expensive to change after more web
platform features depend on them. An accepted decision can have a staged implementation; its ADR
must distinguish the target invariant from the current migration state.

| ADR | Status | Decision |
|---|---|---|
| [0001](0001-renderer-process-boundary.md) | Accepted; top-level renderer-only path implemented | Keep privileged browser services outside an untrusted renderer process |
| [0002](0002-renderer-owned-text-stack.md) | Superseded in implementation | Shape and rasterize untrusted document text inside the renderer with a Rust text stack |
| [0003](0003-lean-renderer-text-pipeline.md) | Accepted | Keep the renderer text boundary with lean Fontique, HarfRust, and Swash adapters |
| [0004](0004-browser-state-and-fetch-broker.md) | Accepted and implemented | Keep persistent state and the streaming Fetch authority in the browser process |
| [0005](0005-boa-runtime-web-api-evaluation.md) | Superseded by 0006 | Keep Web API policy in Breeze; the evaluated `boa_runtime` dependency was not adopted |
| [0006](0006-v8-engine-selection.md) | Accepted and implemented | Replace Boa with V8 while preserving Breeze-owned Web API policy and renderer containment |
