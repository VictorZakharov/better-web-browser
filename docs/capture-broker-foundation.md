# Private capture broker foundation

This change does **not** expose `getUserMedia`, construct a `MediaStream`, activate a camera or
microphone during tests, or earn an HTML5test point. It creates the contained native-capture
boundary that those features can use after the browser has a real permission workflow.

The browser owns the only control pipe. It creates one `Breeze.Capture` AppContainer child for one
grant, carrying only the requested `webcam` and/or `microphone` Windows capability SIDs. The
renderer and existing decoder worker remain capability-free. The child has a one-process,
kill-on-close Job, a restricted child-process policy, an explicit handle allowlist, and
`CREATE_NO_WINDOW`. The child receives a fresh nonce and session ID and will not activate
hardware before the browser proves both and sends `Start`. A grant is one-shot: a stopped or
failed child cannot be reused for another origin, document, or device request.
Before sending `Ready`, the child checks its actual `TokenIsAppContainer`, exact webcam/microphone
`TokenCapabilities`, absence of a console window, and allowlisted environment. The broker checks
that report against its grant and kills the Job on any mismatch.

Samples travel through a separate one-way pipe. Framing has fixed size/version/session/sequence
checks before payload allocation. Video is bounded to 1280×720 NV12 and the newest frame replaces
older preview frames. Audio is bounded to stereo 48 kHz PCM16, split into at most 20 ms packets;
audio queue overrun fails the capture rather than silently shortening its timeline. A blocked
sample pipe has a two-slot writer queue and cannot block `Stop` or `Shutdown` control handling.
The browser has startup and command timeouts and terminates the Job on revocation, protocol
failure, timeout, or drop. A direct process-termination fallback covers Job termination failure.

The native adapter uses Media Foundation device enumeration and Source Readers in the capture
child. It currently chooses the first enumerated camera/microphone, and requires a <=720p native
video mode convertible to NV12 and 16-bit mono/stereo PCM output. These are deliberately narrow
initial formats, not a claim of full constraint support. The camera and microphone Source Readers
have independent timestamps; a future stream consumer must synchronize them. The adapter is only
compile-checked; hardware success and Windows privacy behavior remain unverified.

The browser now has a private, session-only capture admission ledger. Camera and microphone
decisions are separate and keyed by the browser-resolved `Origin`. One-shot tickets bind a
request to tab, document, renderer session, top-level client, request ID, and generation;
foreground and identity are rechecked after prompting and before attaching a session. Navigation,
renderer replacement, tab close/suspend, window deactivation/minimization, and window teardown
retire pending tickets and active leases. Rejected late attachments revoke their lease. Tests
inject counting revokers; the future caller must supply a **nonblocking** broker-Job revoker, not
drop a capture session on the UI thread. No renderer capture request, permission dialog, or real
broker session is connected to this ledger yet.

Before exposing the web API, connect browser-resolved request IPC and a foreground permission UI;
choose a specific device after permission; enforce constraints and Permissions Policy;
deliver tracks and audio/video frames into renderer media elements; stop on track `stop()`,
navigation, permission revocation, renderer death, and window close; and verify hidden end-to-end
capture/preview on actual hardware. A `getUserMedia` shim that returns inert tracks must not be
added.

Primary references: [Media Capture and Streams](https://w3c.github.io/mediacapture-main/),
[Permissions Policy](https://www.w3.org/TR/permissions-policy-1/),
[Media Foundation audio/video capture](https://learn.microsoft.com/windows/win32/medfound/audio-video-capture-in-media-foundation),
[Source Reader media processing](https://learn.microsoft.com/windows/win32/medfound/processing-media-data-with-the-source-reader),
and [Windows capability SID ownership](https://learn.microsoft.com/windows/win32/api/securitybaseapi/nf-securitybaseapi-derivecapabilitysidsfromname).
