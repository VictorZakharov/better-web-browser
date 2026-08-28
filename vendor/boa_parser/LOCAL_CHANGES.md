# Local changes

This directory started from the published `boa_parser` 0.21.1 crate. Its
source revision (`bc36c3fac0969ea21ea0570b62e7846f97389b73`) is recorded in
`.cargo_vcs_info.json`, and the crate retains Boa's `MIT OR Unlicense` terms.

The local delta backports Boa commit
[`029248e4`](https://github.com/boa-dev/boa/commit/029248e4c4bc), which allows
the contextual keyword `of` as a lexical binding in declarations such as
`let of = 1`. ECMAScript permits that declaration, and minified production
JavaScript uses it.

To review the exact patch against an unpacked crate from Cargo's registry:

```powershell
$boaParserRegistry = Get-ChildItem `
  "$env:USERPROFILE/.cargo/registry/src/*/boa_parser-0.21.1" `
  -Directory | Select-Object -First 1 -ExpandProperty FullName
git diff --no-index -- "$boaParserRegistry/src" "vendor/boa_parser/src"
```
