# Fuzzing

The fuzz binaries are deliberately thin wrappers around `better_web_browser::fuzzing`. Every
committed seed is replayed by the stable Windows test suite, while real coverage-guided fuzzing runs
on Linux because `cargo-fuzz` and libFuzzer do not support the native Windows target.

Requirements for coverage-guided runs:

```text
rustup toolchain install nightly-2026-08-15 --profile minimal
cargo install cargo-fuzz --version 0.13.2 --locked
```

Run one target from the repository root:

On Linux x86-64, set `RUSTY_V8_ARCHIVE_SHA256` to
`b6683e9afcb77fbd8cb2c3acf45d34d52029ab216b332522c95179c12f0fed3c` in the shell
environment first. This is the published digest of the pinned V8 152.2.0 Linux release archive;
it overrides the Windows archive checksum in `.cargo/config.toml`. The `Fuzz` workflow sets it
for every job. Keep this pin aligned with the V8 version when upgrading; do not disable verification.

```text
cargo +nightly-2026-08-15 fuzz run html_document fuzz/corpus/html_document -- -max_total_time=60
```

Or run all targets for a bounded interval with PowerShell 7:

```powershell
./scripts/run-fuzz-smoke.ps1 -LibFuzzer -Seconds 60
```

Without `-LibFuzzer`, that script performs the deterministic corpus replay used by pull-request CI.
Coverage-guided campaigns are reserved for scheduled and manually dispatched `Fuzz` runs, where all
six targets run concurrently with pinned tooling. Successful jobs print only seed and final
statistics, while failures expose a bounded log tail and upload the full log and crash artifact. Any
crash, panic, excessive allocation, or five-second input timeout fails the run. Findings must be
minimized, added to the matching corpus, and covered by a stable regression test before the fix is
merged.

Linux campaigns enable symbolized sanitizer reports. `lsan.supp` exempts only the pinned
rusty_v8 allocation `new<v8::isolate::IsolateLiveness>`: upstream deliberately retains this
64-byte liveness cell per isolate so late persistent-handle drops remain safe. Leak detection
stays enabled for all other allocation stacks, and suppression counts are printed. Revisit
this exact exception when upgrading V8. For local runs, set `ASAN_SYMBOLIZER_PATH` to an LLVM
symbolizer executable and `LSAN_OPTIONS=suppressions=/absolute/path/to/fuzz/lsan.supp:print_suppressions=1`.

All checked-in corpus seeds were authored for this repository. Tool licensing and versions are
recorded in `THIRD_PARTY_NOTICES.md`; generated artifacts are ignored and must not be committed.
