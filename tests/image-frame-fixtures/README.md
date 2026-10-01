# Owned image-frame contract probes

All pixels are project-owned tiny color patterns. PNG/APNG and GIF inputs are
encoded with the already-locked MIT/Apache-2.0 `png` and `gif` packages. WebP's
lossless frame payloads come from the existing `image` WebP encoder; the small
test-only RIFF assembly follows the official container specification.
The sixth fixture is an original two-pixel JPEG with a minimal EXIF orientation
record. It verifies that coded sample order and oriented Canvas display differ.

Regenerate with `cargo run --locked --example generate_frame_fixtures --
G:\Git\better-web-browser\target\image-frame-fixtures`, then inspect and update
`frames.json`. The generator refuses non-G: output directories. It is not linked
into or distributed with the browser.

Serve this directory using `scripts/alpha-fixture-server.ps1`. `probe.html` runs
original behavior-level probes in Breeze and unified-headless Chromium. It checks
actual samples, composition, timing and ownership—not constructor presence.
All requests are confined to the owned loopback server; no public sites or saved
profiles are needed. Generated captures belong in G: target directories, not Git.
