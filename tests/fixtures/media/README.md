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

## Ordinary video-only MP4

`test-1s-video.mp4.base64` is a nonfragmented stream-copy remux of the same
BSD-3-Clause WPT fixture. It exercises the Media Foundation Source Reader path
for complete, ordinary H.264-only MP4 files, separately from the fragmented
track path above. It was produced with the same development-only FFmpeg build:

```text
ffmpeg -i test-1s.mp4 -map 0:v:0 -c copy -movflags +faststart video-only.mp4
```

The decoded file is 12,366 bytes with SHA-256
`52b9685901be7572653e088cfd13eaa62e49d552efa2db8c77d667ce5490629f`.

## MP3 audio fixture

`test-1s-audio.mp3.base64` is a 64 kb/s MP3 re-encode of the BSD-3-Clause
WPT `test-1s-audio-fragmented.mp4.base64` fixture above. It was produced
with the locally installed FFmpeg `N-111280-gd51b0580e4-20230625` using
`-map 0:a:0 -c:a libmp3lame -b:a 64k -f mp3`. FFmpeg is a development-only
conversion tool, not linked into or distributed with Breeze. The decoded MP3
is 8,712 bytes and has SHA-256
`41fecf66af79b153fa5053cdef47a7a0f860fd968e29a7bbf723f110733306b1`.

`test-1s-audio.aac.base64` is a stream-copy ADTS remux of the same WPT AAC
track, produced with `-map 0:a:0 -c:a copy -f adts` using that development-only
FFmpeg build. The decoded ADTS file is 603 bytes and has SHA-256
`22ff76c842f90ab11f582eb98b7e0135fa0affd9186033f47bb3b258383c155b`.

## Self-authored native FLAC audio fixture

`test-1s-audio.flac.base64` encodes a one-second 440 Hz mono sine wave generated
from a synthetic signal, not a third-party recording. The output is native FLAC
(`fLaC` signature), not Ogg or Matroska. It was generated with the locally
installed FFmpeg `N-111280-gd51b0580e4-20230625`:

```text
ffmpeg -hide_banner -loglevel error -f lavfi -i sine=frequency=440:sample_rate=44100:duration=1 -ac 1 -c:a flac -y test-1s-audio.flac
```

The decoded file is 20,333 bytes with SHA-256
`dd8080cb04e28222c585c552e6f76a6669c789aeb621b802ecfe728949adc7ad`.
FFmpeg is a development-only fixture generator, not a runtime, build, or test
dependency. The fixture tests decoding, play/pause, and seeking through the
pure-Rust FLAC decoder in the contained media worker, independent of an
installed Windows Media Foundation FLAC codec.

## Self-authored Web Audio MP3 and AAC-in-M4A fixtures

`test-0.4s-tone.mp3.base64` and `test-0.4s-tone.m4a.base64` encode the same
0.4-second 440 Hz mono synthetic sine wave, not a third-party recording.
They exercise MP3 and AAC-in-ISO-BMFF `decodeAudioData` and contained media
playback paths. The locally
installed FFmpeg `N-111280-gd51b0580e4-20230625` generated them with:

```text
ffmpeg -hide_banner -loglevel error -f lavfi -i 'sine=frequency=440:sample_rate=44100:duration=0.4' -ac 1 -c:a libmp3lame -b:a 96k -y synthetic-tone.mp3
ffmpeg -hide_banner -loglevel error -f lavfi -i 'sine=frequency=440:sample_rate=44100:duration=0.4' -ac 1 -c:a aac -b:a 96k -movflags +faststart -y synthetic-tone.m4a
```

| Decoded file | Bytes | SHA-256 |
| --- | ---: | --- |
| MP3 | 5,686 | `1180594624bafb2d762b80dab5c219c263a42d164d99312b9c6a06402d3bf24f` |
| M4A | 6,076 | `b6ee3c425b03eb330de086c518485057ec41f4f8b52878fbc91e27c3daacdd43` |

FFmpeg is a development-only fixture generator, not a runtime, build, or test
dependency. The Base64 encoding keeps both binary fixtures reviewable.

## Self-authored Ogg/Vorbis audio fixture

`test-2s-audio.ogg.base64` is a two-second 440 Hz mono sine wave generated
from a synthetic signal, not a third-party recording. It was produced with
the same development-only FFmpeg `N-111280-gd51b0580e4-20230625`:

```text
ffmpeg -hide_banner -loglevel error -f lavfi -i sine=frequency=440:sample_rate=44100:duration=2 -ac 1 -c:a libvorbis -q:a 2 -y test-2s-audio.ogg
```

The decoded fixture file is 6,675 bytes with SHA-256
`1e65839c935c43c481f9e7a7df3888a2ab2ce9e9fb05f5d0b86c846f536e4a7b`.
FFmpeg is not a runtime, build, or test dependency. The fixture exercises the
pure-Rust Vorbis decoder and silent playback in the contained media worker.
