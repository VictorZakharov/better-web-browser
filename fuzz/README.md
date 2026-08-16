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

```text
cargo +nightly-2026-08-15 fuzz run html_document fuzz/corpus/html_document -- -max_total_time=60
```

Or run all targets for a bounded interval with PowerShell 7:

```powershell
./scripts/run-fuzz-smoke.ps1 -LibFuzzer -Seconds 60
```

Without `-LibFuzzer`, that script performs the stable corpus replay used by pull-request CI. The
scheduled `Fuzz` workflow runs all six targets with pinned tooling. Any crash, panic, excessive
allocation, or hang must be minimized, added to the matching corpus, and covered by a stable
regression test before the fix is merged.

All checked-in corpus seeds were authored for this repository. Tool licensing and versions are
recorded in `THIRD_PARTY_NOTICES.md`; generated artifacts are ignored and must not be committed.
