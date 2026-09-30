# Ordinary wheel-input measurement

The hidden `--wheel-after-ready x,y,delta` path sends the ordinary document-owned
wheel input. It does not call `scrollTo()` to bypass renderer dispatch, event
listeners, default-action cancellation, or nested scroller selection.

The benchmark's bounded `wheel_input_trace` separates three endpoints:

| Field | Meaning |
| --- | --- |
| `enqueue_to_decision_received_ms` | Browser enqueue to receipt of the renderer's owning report; includes queueing, wheel work, post-dispatch style/layout and report assembly/serialization, outbound IPC and browser delivery |
| `renderer_dispatch_ms` | Renderer-local `wheel_input` span, including its event/default-action work and any layout performed there; excludes later `presentation_after_user_input` style/layout and serialization |
| `enqueue_to_first_paint_ms` | Browser enqueue to the first hidden retained paint of the resulting native motion; not monitor scanout |
| `first_paint_path` | `full_retained` or `exposed_strip`, or null when no motion paint is attributed |

No subtraction between process clocks is used. These spans have different work
boundaries: decision-receipt minus renderer-dispatch time is not pure queue
latency, because it also includes presentation and transport work. A cancelled or no-motion input
has null motion-paint latency, not zero. Retired documents, superseded inputs,
unacknowledged inputs, rejected input and paint failures remain explicit outcomes.
The trace retains at most 128 samples and reports omitted inputs and unmatched
acknowledgements rather than silently discarding them.
Native motion ownership is resolved before the queue's synchronous first tick.
A viewport verdict can finish as `no_motion` when opposite coalesced deltas cancel,
fractional conversion produces no device-pixel distance, or range clamping keeps
the previous target. An existing animation alone cannot supply a paint latency
for that new request; its synchronous and later timer ticks remain unattributed.

## Ownership and visual parity

Renderer verdicts carry the input sequence and travel in a report owned by one
document. Input sequence and document fences still apply before dispatch.
Ordered report compaction preserves verdicts and opposite wheel deltas; an
owning nested-scroll presentation is a barrier so a later unrelated resource
snapshot cannot be credited with its motion.

The nested verdict describes the wheel's default-action offset in its owning
presentation. CSSOM View scroll notifications are queued and coalesced; they do
not run synchronously inside that default action. A later scroll listener may
restore the offset and change a color. That separate snapshot has no new wheel
verdict and cannot establish or overwrite the earlier input's motion-paint
latency. The initial moved snapshot can legitimately have a nested motion paint
even though the final settled offset returns to zero. Background-tab reports
cannot retire foreground measurements or paint the foreground benchmark surface.

Hidden paints follow the ordinary retained-paint invalidation rules. Sticky
position changes and newly installed visual content invalidate cached pixels
before a wheel paint; unchanged scrolling may reuse pixels and repaint only the
exposed strip. Reporting a cheap strip paint of stale content is not a valid
performance improvement.

The report's new wire field changes renderer IPC major version to 15. Browser
and renderer must come from the same rebuilt executable; incompatible older
major versions are rejected before decoding their different report layout.

## Evidence and limits

Owned unit and hidden fixtures cover cancellation, nested scrolling, direction
reversal, stale/coalesced sequences, asynchronous offset restoration, prior long
renderer work, sticky position changes and listener-driven visual mutations.
These tests establish what the counters mean, not that a live site is fast.

The existing direct `early_scroll_trace` remains a useful retained-paint probe,
but excludes the wheel dispatch round trip. Its p95 must not be described as
native wheel latency. Likewise, a Chrome animation-frame or compositor-delivery
measurement is not the same endpoint as Breeze's retained native paint. Report
each endpoint and its transport overhead rather than deriving a speed ratio.

## Chromium native-wheel diagnostic

The Chromium harness accepts repeated `--wheel-after-ready x,y,delta` options.
It sends [CDP `Input.dispatchMouseEvent`](https://chromedevtools.github.io/devtools-protocol/tot/Input/#method-dispatchMouseEvent)
with `type: mouseWheel`, CSS viewport coordinates, `deltaX: 0` and the requested
pixel `deltaY`. Native-wheel runs reject the direct `--early-scroll` and
`--scroll-samples` probes, and do not execute the final `scrollTo(0, 0)` reset.
They also reject combined navigation/editing actions so one owned document is
measured. Launch settings retain `--headless`, `--mute-audio`,
`CreateNoWindow = true`, fresh profiles by default, and a visible-window check.

The `native_wheel` report keeps all admitted samples in input order:

| Field | Meaning |
| --- | --- |
| `enqueue_to_cdp_reply_ms` | Host enqueue to successful CDP reply; CDP does not document this as the renderer's default-action verdict |
| `enqueue_to_first_changed_compositor_frame_received_ms` | Host enqueue to receipt of the first direction-consistent viewport scroll-offset frame; includes PNG generation and CDP delivery |
| `frame_swap_timestamp`, `frame_swap_monotonic_timestamp` | Optional raw screencast source clocks; never subtracted from host timestamps |
| `scheduled_ms`, `enqueued_ms` | Requested and actual host action times, so a delayed dispatch is visible |

The frame source is [`Page.screencastFrame`](https://chromedevtools.github.io/devtools-protocol/tot/Page/#event-screencastFrame)
metadata, not a DOM scrolling shortcut or monitor scanout. Opposite-direction
frames cannot satisfy a reversed delta. A same-direction offset change can still
include prior in-flight motion; this is an observed frame-receipt endpoint, not
a proof of exclusive compositor attribution to one wheel. Leave enough spacing
for prior motion to settle and retain the raw per-input records.

A passive listener in a [separate isolated world](https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-createIsolatedWorld)
records the event's cancellation flag after dispatch. It does not install a page
main-world global, call `preventDefault()`, or change scrolling. The observer is
diagnostic overhead, not ordinary platform support. Missing listener verdicts,
cancelled events and nested-scrollport candidates have null attributed frame
latency. Main-frame scroll metadata cannot prove which nested pixels were painted.
`no_viewport_motion_observed` is not inferred cancellation: exhausted scroll
ranges, zero deltas, or unavailable frame evidence can all produce it. Retired,
superseded, unacknowledged and failed inputs also remain explicit. Records are
bounded to 128; requested, unattempted and omitted counts are reported.

One screencast owner acknowledges each frame. File filmstrip sampling may end
before a settled wheel sequence; native-wheel observation keeps the same stream
alive until that sequence completes. This instrumentation adds capture/transport
work, so it must not be equated with Breeze's retained-paint counter or used for
a browser speed ratio.

## Matched geometry invocation

Use release outputs rebuilt from the same final source state. Keep `TEMP`, `TMP`,
profiles, .NET build outputs and captures on G:. For example, build the package-free
Chromium harness with `dotnet build benchmarks/chromium/ChromiumBaseline.csproj
-c Release --artifacts-path G:/Git/better-web-browser/target/wheel-measurement/dotnet`.
The pure `ChromiumBaseline.Tests` self-tests support `--wheel-timing-only`; that
switch exits before any browser launch.

The example below assumes the harness DLL, release Breeze executable and artifact
directory already exist. It uses CSS viewport `1249x548`, scale `1.25`, `en-US`,
eight deltas at `(600,300)` and 1000 ms spacing. Breeze's outer window is
`1280x720`; verify its reported content viewport before comparing. Set
`$firstDelay` to `1000` for an early sequence or `12000` for a settled sequence.
The delay is relative to each harness's own page-ready point: first owned layout
and paint in Breeze, `Page.loadEventFired` in Chromium. These are different readiness
endpoints, so retain the ready timestamps rather than implying synchronized starts.
The default initial delay is zero and preserves normal navigation-delay behavior;
a nonzero value changes only the first scheduled action, not every later interval.

```powershell
$artifact = 'G:/Git/better-web-browser/target/wheel-measurement'
New-Item -ItemType Directory -Force -Path "$artifact/temp" | Out-Null
$env:TEMP = "$artifact/temp"
$env:TMP = "$artifact/temp"
$firstDelay = 12000
$targets = 600,600,600,-600,-600,-600,600,-600 | ForEach-Object { "600,300,$_" }
$url = 'https://example.org/long-document'
./scripts/run-hidden-benchmark.ps1 -Url $url -Output "$artifact/breeze.json" `
  -Browser 'G:/Git/better-web-browser/target/release/better-web-browser.exe' `
  -FreshProfile -WindowWidth 1280 -WindowHeight 720 -DeviceScaleFactor 1.25 `
  -Locale en-US -WheelTarget $targets -NavigationDelayMs 1000 `
  -InitialActionDelayMs $firstDelay -SettleMs 2000
$wheelArgs = @()
foreach ($target in $targets) { $wheelArgs += '--wheel-after-ready', $target }
dotnet "$artifact/dotnet/bin/ChromiumBaseline/release/ChromiumBaseline.dll" `
  --url $url --output "$artifact/chromium.json" `
  --viewport-width 1249 --viewport-height 548 --device-scale-factor 1.25 `
  --locale en-US --navigation-delay-ms 1000 --initial-action-delay-ms $firstDelay `
  --settle-ms 2000 --timeout-ms 30000 @wheelArgs
```

For deterministic fixture checks, serve `tests/fixtures` with
`scripts/serve-alpha-fixtures.ps1 -Root G:/Git/better-web-browser/tests/fixtures
-ReadyFile <G: marker path>` in an owned hidden process. Its ready marker publishes
the loopback base URL; append `wheel-timing.html`, `wheel-timing-reset.html`,
`wheel-timing-sticky.html` or `wheel-timing-mutation.html`. Cancellation and nested
targets on the first fixture are `(300,50)` and `(50,50)` respectively; the matched
`(600,300)` target exercises the viewport. Stop that owned server after testing.
Do not run performance samples during competing builds or treat these fixture
contracts as live-site timing claims.

The HTML5test score is a capability inventory. Time-to-visible-score needs
filmstrip evidence; HTTP success, a document title or a fast scroll-paint counter
does not establish a visible result or responsive wheel input.

See [UI Events wheel behavior](https://www.w3.org/TR/uievents/#events-wheelevents),
[DOM event-listener cancellation](https://dom.spec.whatwg.org/#observing-event-listeners),
and [CSSOM View scrolling](https://drafts.csswg.org/cssom-view/#scrolling).
