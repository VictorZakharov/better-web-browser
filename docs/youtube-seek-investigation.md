# YouTube seek investigation: current evidence

Status on September 12, 2026: **not resolved**. The generic media/input/network
corrections in this branch do not establish reliable live YouTube seeking.
Earlier experiments are retained in [media-source-seeking.md](media-source-seeking.md).

## A working reference, not a failing automated baseline

The user reports successful mouse seeks in regular Chrome, Incognito and Guest.
An explicitly approved, muted visible Guest comparison reproduced that success
with installed Chrome 152.0.7977.83 and an isolated temporary profile. No personal
profile was opened or copied, and no browser APIs were overridden.

The reference used a nonzero loopback debugger port without headless or
enable-automation switches. Its document was visible/focused and
`navigator.webdriver` was false. Mouse input activated the player's midpoint
after ten seconds of playback. This one-off visible comparison is not a change
to the repository's hidden/silent automation policy.

| Observation | Working Chrome Guest reference | Breeze response-probe run |
| --- | --- | --- |
| Input | Mouse midpoint after ten seconds of playback | Trusted `k`, then `5` after ten seconds |
| Seek target | 601.533 s | 601.533 s |
| Post-seek advancement | Observed by 744.476 ms; continued to 626.152 s | None; player reset to its error screen |
| Frames | 1,196 total, including post-seek playback | 298, all before the seek |
| Refill | First post-seek POST transferred 4,031,151 encoded bytes over HTTP/3 | Small response bodies with repeated stream-protection statuses |
| Browser / profile cleanup | Completed | Completed |

The 744 ms observation is a conservative sampled advancement test, not exact
input-to-first-pixel latency: it requires time above 601.8 s, additional frames,
`seeking === false`, and continued playback, sampled about every 200 ms.
This is not a repeated-run percentile or a controlled performance ratio between
the two browsers. Audio was muted; physical audio output was not assessed.

A separate headless Guest mouse comparison stalled at the same position. It
reported `navigator.webdriver === true` and completed browser/profile cleanup.
[Chromium's runtime-feature initialization](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/content/child/runtime_features.cc)
enables its automation indicator for headless mode or debugger port zero; a
nonzero debugger port alone does not enable it. This identifies a confound in
earlier visible comparisons that used port zero, **not proof that this flag
alone causes failure**. The configurations differ in multiple respects.

Failing automated Chrome runs must not be treated as equivalent to the working
reference or used to dismiss Breeze's failure. The successful capture also
contains small control responses, so response size alone is not an error test.

## What the failing refill contains

A temporary observer at Breeze's Fetch-delivery boundary left all response bytes
and return values unchanged. It retained at most 2 KiB per small response in
memory and emitted only bounded framing types, lengths and selected numeric
status fields. No raw bodies, tokens, signed URLs or headers were saved.
Both the Window and Worker delivery paths were instrumented in a separately
named probe executable; no renderer permissions or network policy were changed.

| Observed small-response sequence | Decoded body length | Numeric fields |
| --- | --- | --- |
| First two reports | 192 bytes each | Part 58, status 2 |
| Next report | 168 bytes | Part 58, status 3, max retries 10 |
| Following nine reports | 170 bytes each | Part 58, status 3, max retries 10 |

The independently written diagnostic parser was checked with synthetic input
for framing, unchanged delivery, omission of string fields, size limits and
abort cleanup. It is not shipped as a browser feature or protocol dependency.

The [upstream stream-protection schema](https://github.com/LuanRT/googlevideo/blob/main/protos/video_streaming/stream_protection_status.proto)
names these fields `status` and `max_retries`;
[that implementation](https://github.com/LuanRT/googlevideo/blob/main/src/core/SabrStream.ts)
interprets status 2 as attestation pending and status 3 as attestation required.
This is a third-party protocol implementation, **not an official YouTube
specification**. The numeric observations are direct evidence; the attestation
interpretation depends on that source.

This directs the next investigation toward the site's request/session and
browser-API behavior before media reaches the decoder. It does not establish
which missing API, event-ordering defect, environment property, or server
decision causes the failure. It is not evidence for increasing a decoder
timeout, claiming HTTP/3 will fix it, or adding a site-specific retry policy.

A repeat with a temporary native promise-rejection observer also failed (297
pre-seek frames) and produced the same protection statuses. The observer was
first validated on an owned loopback page containing two intentional rejected
promises; both were observed, including a native missing-property TypeError.
No such rejection was observed on the live page. This does not rule out errors
caught synchronously or handled internally by the site's own promise machinery,
and is not a claim that the page's JavaScript or all browser APIs are correct.

A final bounded request-shape observation found the nested context field in
all 16 inspected media POSTs. The first two contained a 10-byte attestation
field; the remaining 14 contained an 81-byte field, including requests whose
responses reported protection status 3. The nested field numbers come from
the same upstream [request](https://github.com/LuanRT/googlevideo/blob/main/protos/video_streaming/video_playback_abr_request.proto)
and [context](https://github.com/LuanRT/googlevideo/blob/main/protos/video_streaming/streamer_context.proto)
schemas. Only presence, lengths and request IDs were retained; values were not
printed, persisted, altered or replayed. Synthetic cases covered present,
absent, truncated and over-limit input while preserving the original host call.

That run still failed (295 pre-seek frames, no post-seek advancement). This
rules out simple absence of that field in these requests, not invalid contents,
wrong session binding, a provisional result or another verification failure.
It does not prove that the site successfully completed attestation. All
temporary production hooks were removed; these findings change documentation,
not runtime behavior.

## Acceptance and limits

Any proposed engine correction needs a reduced, standards-based regression
before it is attributed to this incident. Retest the normal player after actual
initial playback, with a far-ahead seek, new post-seek video frames, advancing
audio/video time, continued refill and screenshots. Starting at a timestamped
URL, an HTTP 200 response, pre-seek frame totals, or a successful harness process
alone does not meet that acceptance test.

No token generation/replay, browser-identity spoofing, site-specific protocol
implementation, security-policy relaxation or increased decoder limits are
proposed by these observations.
