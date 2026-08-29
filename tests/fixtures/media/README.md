# Owned media fixture

`test-1s.mp4.base64` is an unmodified Base64 representation of
`media/test-1s.mp4` from web-platform-tests revision
`322ebb726e0bc6ee05c5635f2978e3175dd781b9`.

- Upstream: <https://github.com/web-platform-tests/wpt/blob/322ebb726e0bc6ee05c5635f2978e3175dd781b9/media/test-1s.mp4>
- Decoded file length: 13,932 bytes
- Decoded SHA-256: `dc72b1b5591bbc9e2d0d6b511fa6d5134dd78dca6cf357244d656225f62a94b5`
- License: BSD-3-Clause; see `LICENSE-WPT.md`

The text encoding keeps the binary fixture reviewable through the repository patch workflow. Tests
decode it in memory and transfer the resulting bytes over the production media data framing. It is
not included in release behavior or accepted from pages.
