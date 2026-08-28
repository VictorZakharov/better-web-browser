# Local changes

This directory started from the published `boa_engine` 0.21.1 crate, whose
source revision is recorded in `.cargo_vcs_info.json`.

The local delta is intentionally small:

- `JsObject::try_downcast_mut` was added and the ordered Map/Set finalizers use
  it so finalization does not panic when a JavaScript object is already mutably
  borrowed.
- `HashMap::get_many_mut` was updated to `get_disjoint_mut` for the dependency
  API used by this build.
- A function-pointer comparison uses explicit pointer casts to satisfy current
  Rust diagnostics.
- One documentation-only trailing-whitespace warning was removed.
- The Stage 4 `Error.isError` implementation is enabled independently of Boa's
  broader experimental feature set, so Web IDL `DOMException` objects retain
  their required Error identity without enabling unrelated proposals.
- Native errors capture their JavaScript backtrace before entering a local
  `catch` block, and the resulting opaque Error exposes the de-facto `stack`
  property. This preserves actionable source locations for caught failures in
  site code as well as uncaught exceptions.

To review the exact patch against an unpacked crate from Cargo's registry:

```powershell
$boaRegistry = Get-ChildItem `
  "$env:USERPROFILE/.cargo/registry/src/*/boa_engine-0.21.1" `
  -Directory | Select-Object -First 1 -ExpandProperty FullName
git diff --no-index -- "$boaRegistry/src" "vendor/boa_engine/src"
```
