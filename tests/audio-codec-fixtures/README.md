# Owned audio contracts

These fixtures are original project code with synthetic PCM generated in memory;
they do not download audio, access microphones or require playback devices.

Serve this directory through the existing alpha fixture server and open
`probe.html` only through hidden automation. Prefer `scripts/test-audio-codecs.ps1`
for both Breeze and Chromium: it controls profiles, artifact location, process
visibility and fail-closed completion checks.

Each row publishes a contract name, status and failure detail. The summary's
`data-passed` and `data-total` appear only after every selected probe finishes.
A timed-out browser is not a successful or completed capability test.

The default runs all seventeen contracts. The `begin`/`end` query parameters select
a half-open range for investigating unsupported or stalled reference paths.
They are deliberately visible in the acceptance report. Do not use a reduced
range to claim that the complete fixture passes.

Sample conversion has its own exact unit tests. The shared browser fixture allows
one integer destination LSB when narrowing from signed 32-bit input, because
Chromium may first quantize through float32. Opus round-trip checks decoded energy
and finite samples rather than lossy bit equality. Flush padding is observable.

The final PCM probe verifies `pcm-s24` signed expansion. The tested Chromium build
did not finish it within the timeout; isolate that probe when comparing the rest,
and retain the failure in any reported comparison. The explicit `ogg` configuration
probe follows the registered OpusHead contract, even if a reference rejects it.

See [the implementation contract](../../docs/webcodecs-audio.md) for admitted
configurations, queue/lifetime limits, provenance and remaining work.
