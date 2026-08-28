# Local changes

This directory started from the published `boa_ast` 0.21.1 crate. Its source
revision (`bc36c3fac0969ea21ea0570b62e7846f97389b73`) is recorded in
`.cargo_vcs_info.json`, and the crate retains Boa's `MIT OR Unlicense` terms.

The local delta backports two fixes that Boa merged after 0.21.1:

- [`acbfd726`](https://github.com/boa-dev/boa/commit/acbfd7261ba49b734473e1be42ed9d24ab3809c3)
  forces the runtime function-scope layer used by class constructors.
- [`46e92c75`](https://github.com/boa-dev/boa/commit/46e92c7507e4b14f9511986e04333c16863280d0)
  applies the same invariant to class-expression constructors and static blocks.

Without these fixes, bytecode can address lexical bindings in an environment
that the scope-index optimizer omitted, terminating a document runtime with an
out-of-bounds panic.

It also preserves the nearest non-arrow function environment when `this` is
captured through multiple nested arrow functions. Boa 0.21.1 stopped at the
first enclosing arrow scope, which can optimize away the real `this` owner and
misaddress destructured callback parameters.

To review the exact patch against an unpacked crate from Cargo's registry:

```powershell
$boaAstRegistry = Get-ChildItem `
  "$env:USERPROFILE/.cargo/registry/src/*/boa_ast-0.21.1" `
  -Directory | Select-Object -First 1 -ExpandProperty FullName
git diff --no-index -- "$boaAstRegistry/src" "vendor/boa_ast/src"
```
