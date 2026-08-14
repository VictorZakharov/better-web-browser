# Architecture decisions

Architecture decision records document boundaries that are expensive to change after more web
platform features depend on them. An accepted decision can have a staged implementation; its ADR
must distinguish the target invariant from the current migration state.

| ADR | Status | Decision |
|---|---|---|
| [0001](0001-renderer-process-boundary.md) | Accepted; staged implementation | Keep privileged browser services outside an untrusted renderer process |
