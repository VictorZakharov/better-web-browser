# Breeze 0.1.0 public technical alpha

This is an unsigned, Windows x64-only technical alpha for experimentation with Breeze's independently owned browser engine. It is not a security-audited general-purpose browser.

## Acceptance evidence

- The [initial public-alpha comparison](https://github.com/VictorZakharov/better-web-browser/blob/main/docs/alpha-compatibility.md) recorded 27 passing Breeze/Chromium pairs on 2026-08-24. This is historical evidence for the then-nine-fixture matrix, not the current twelve-fixture gate or the performance of a newly packaged executable.
- The [curated WPT gate](https://github.com/VictorZakharov/better-web-browser/blob/main/tests/wpt/README.md) documents the current pinned selection and reproduction commands. The initial alpha's 80 files / 570 subtests are historical, not today's coverage. Both former discovery cases have graduated into the strict gate; the documented remaining boundaries still prevent treating it as a whole-platform conformance score.
- The [hostile-input and fuzz policy](https://github.com/VictorZakharov/better-web-browser/blob/main/docs/security-and-fuzzing.md) covers checked-in deterministic corpora on every source change and scheduled bounded libFuzzer campaigns. Current workflow status is available under [Fuzz](https://github.com/VictorZakharov/better-web-browser/actions/workflows/fuzz.yml).
- Release smoke launches the packaged executable with Rust/Cargo paths removed, creates a new isolated profile, navigates a loopback page through an AppContainer renderer, verifies cookie and local-storage persistence, exercises renderer crash/reload recovery, and removes the portable test installation.

## Download and verify

1. Download `breeze-v0.1.0-x86_64-pc-windows-msvc.zip` and `SHA256SUMS.txt` from the release.
2. In PowerShell, run `Get-FileHash .\breeze-v0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256` and compare the lowercase value with `SHA256SUMS.txt`.
3. Extract the archive and run `better-web-browser.exe`. A Rust toolchain is not required.

The archive contains the executable, project license, complete locked-graph notices and package license files, these release notes, `VERSION.txt`, and `artifact-manifest.json`. The external manifest and checksum file are published beside it. The manifest explicitly records `signed: false`; no code-signing claim is made.

## Data and portable cleanup

Normal launches store cookies and `localStorage` under `%LOCALAPPDATA%\Breeze`. Removing the extracted program directory uninstalls the portable application. To remove its browsing data too, close Breeze and delete that exact profile directory; this permanently removes its stored site data. `sessionStorage` is never persisted. There is no installer registration, service, shell extension, or automatic updater in this alpha.

## Known limitations and safety boundary

- Windows 10/11 x64 only; there is no MSI in this release.
- The binary is unsigned and may trigger Windows reputation warnings.
- Platform coverage is incomplete. A bounded Canvas 2D pixel slice and user-visible non-DRM H.264/AAC MP4 playback, Media Source input, synchronized audio, controls, and fullscreen are implemented. Other Canvas drawing operations, codecs and containers, encrypted media, captions, track selection, picture-in-picture, downloads, extensions, and cross-site frame isolation remain incomplete; many broader WPT areas fail.
- Windows UI Automation exposes browser chrome and a bounded active-document semantic tree, but accessible-name/ARIA coverage, rich text patterns, live regions, Reader semantics, and non-Windows adapters remain incomplete. See [Accessibility architecture](accessibility.md).
- Each tab has a capability-free AppContainer renderer and browser-owned recovery surfaces, but Breeze has not received an independent security audit and does not provide complete site isolation.
- Do not use this alpha for banking, password-manager access, sensitive authenticated browsing, or other high-value sessions. Expect rendering defects, crashes, data loss, and incompatible profile changes.
- Live-site behavior and benchmark numbers vary. The published medians apply only to the controlled feature-equivalent fixtures and are not a universal speed or compatibility claim.

## Reproduction and release authority

Before publishing a new snapshot, rebuild the exact release commit and rerun the curated WPT,
deterministic alpha matrix, and packaged release smoke. Retain their JSON reports and generated
environment metadata. Any refreshed HTML5test observation must include a dated commit, command,
fresh-profile/viewport conditions, diagnostics, and screenshot; never infer support from its score.
Audit README counts against the manifests/reports and label older measurements historical rather
than silently carrying them forward. Verify linked documentation and commands with the package.

Release builds use the checked-in Rust 1.95.0 toolchain, locked dependencies, canonical Cargo `release` profile, Windows x64 target, deterministic archive ordering/timestamps, and commit/version metadata. `scripts/package-technical-alpha.ps1` produces the archive twice in CI and requires identical SHA-256 hashes.

The manual release workflow accepts only an existing `v<package-version>` tag whose commit is on protected `main`. It cannot create a tag, merge a pull request, bypass branch protection, or publish a release: it creates or refreshes a draft that the repository owner must publish explicitly.

[dist 0.32.0](https://github.com/axodotdev/cargo-dist/releases/tag/v0.32.0) was evaluated. Its generated multi-stage release/installer workflow is larger than this single-target portable path and does not own Breeze's profile/AppContainer/crash/cleanup acceptance contract. The focused scripts are therefore the smaller reviewable fallback. An MSI is deferred until product identity, upgrade/uninstall behavior, and real code signing can be tested without making a misleading trust claim. [cargo-deny 0.20.2](https://github.com/EmbarkStudios/cargo-deny/releases/tag/v0.20.2) remains the maintained CI policy engine for advisories, licenses, bans, and sources; there are no ignored advisories, license exceptions, allowed Git sources, or unknown registries. The production JavaScript engine is the exact locked `v8` crate version, and its license provenance is included in the generated third-party notices.
