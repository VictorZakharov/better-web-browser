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

## Separate fragmented tracks

`test-1s-video-fragmented.mp4.base64` and `test-1s-audio-fragmented.mp4.base64` are
stream-copy remuxes of that same BSD-3-Clause fixture, not downloaded YouTube media.
They exercise independent H.264/AAC appends and their actual buffered extents.

Generated with the locally installed FFmpeg `N-111280-gd51b0580e4-20230625`:

```text
ffmpeg -i test-1s.mp4 -map 0:v:0 -c copy -movflags frag_keyframe+empty_moov+default_base_moof video.mp4
ffmpeg -i test-1s.mp4 -map 0:a:0 -c copy -movflags frag_keyframe+empty_moov+default_base_moof audio.mp4
```

No codec is re-encoded, linked, or redistributed; FFmpeg is a development-only remuxing tool
(the installed build reports GPL-3.0-or-later), not a build/test/runtime dependency.
The fixture content retains the upstream BSD-3-Clause license in `LICENSE-WPT.md`.

| Decoded file | Bytes | SHA-256 |
| --- | ---: | --- |
| Video track | 12,345 | `f9a4878d6826eb8691299c19a3e188805471dd54ba1cca4b9bd064d14c7a515f` |
| Audio track | 1,563 | `7448850fd2861adaf9a26bc286ca53d80bca7ad18f4807c3b03e4563192abfdb` |
