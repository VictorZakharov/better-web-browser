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

Per-verdict retained viewport distance changes renderer IPC major version to 16. Browser
and renderer must come from the same rebuilt executable; incompatible older
major versions are rejected before decoding their different report layout.

### Reversal while renderer replies are backlogged

The shell observes vertical direction at native input submission, before IPC
admission. A reversal cancels the running native animation and its fractional
residue immediately. A per-tab sequence fence retires older *viewport default
distance*, not the DOM wheel events themselves. Their cancellation verdicts,
nested-scroll snapshots, console output and author-written absolute scrolls
still install normally. A cancelled or rejected reverse still interrupts old
travel; it cannot authorize a new viewport default. Content input rejection is
distinct from input outside the content surface and never takes raw-scroll
fallback. Raw browser/reader scrolling also reserves a fence sequence. Tab
suspension retires outstanding defaults before returning to that tab.

Each acknowledgement carries its retained accepted CSS distance. Compaction
clears older contributions when a newer absolute scroll resets the anchor,
while keeping incoming relative defaults after that anchor. This lets the
shell discard only pre-reversal contributions, including two rapid reversals,
without losing valid distance combined in the same report.

An owned fixture exposed the remaining failure in release head `6f562c1`:
eight forward inputs and a reverse were submitted while the first wheel
listener ran a real bounded 100 ms task. Reverse submission occurred around
13 ms and the first verdict around 100 ms. All nine trusted DOM events arrived,
but old native motion restarted after the reversal in both directions. Even
the cancelled reverse settled at 3,992/6,008 CSS pixels instead of the unchanged
5,000-pixel starting position. Those four raw baseline reports were retained;
this is correctness evidence, not an uncontended latency measurement.

The regression checks the whole native position curve after reversal, ordered
trusted DOM delivery, cancellation, and the final renderer scroll feedback.
Checking only the reverse event's first paint would miss this backlog failure.
The interruption guarantee begins when the UI receives the wheel input; a
synthetic hidden submission does not measure hardware-to-message-queue delay.

### Native queue fairness and continuous frames

The message pump prefers at most eight actual input-queue messages before an
ordinary queue read. It does not discard, compact, or reorder the admitted DOM
wheel sequence. Posted translated characters, including surrogate pairs and
dead/system characters, retain ordinary queue order before later keys or focus
changes. Renderer output yields between atomic events after a soft 4 ms
turn, or when native input is waiting; deferred output retains FIFO order and
terminal recovery cannot overtake it. One expensive event can exceed that soft
budget, so it is not a hardware-input latency guarantee.

Win32 timer messages have low queue priority. After a completed dispatch, due
renderer-monitor work and a due foreground scroll frame can therefore run
through their existing paths without waiting for a timer message. Both use real
elapsed time and retain their deadlines across unchanged input; no repeated
`SetTimer` calls or synthetic per-input frame ticks are introduced. A queued
timer arriving immediately after serviced work cannot duplicate that work.
Same-direction inputs extend one animation clock; reversal cancels its old
target before new renderer authorization is available.

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
| `listener_verdict_reason`, `observed_listener_events`, `observed_event_delta_y` | Exact sequence/delta observation or the reason it remains unknown; missing values are not zero |
| `listener_verdict_deadline_reached` | No admitted final listener verdict arrived within the single observation budget |

The frame source is [`Page.screencastFrame`](https://chromedevtools.github.io/devtools-protocol/tot/Page/#event-screencastFrame)
metadata, not a DOM scrolling shortcut or monitor scanout. Opposite-direction
frames cannot satisfy a reversed delta. A same-direction offset change can still
include prior in-flight motion; this is an observed frame-receipt endpoint, not
a proof of exclusive compositor attribution to one wheel. Leave enough spacing
for prior motion to settle and retain the raw per-input records.

A passive listener in a [separate isolated world](https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-createIsolatedWorld)
retains at most 128 event references and snapshots each cancellation flag after
[listener dispatch](https://dom.spec.whatwg.org/#concept-event-dispatch), without
depending on a separately scheduled timer. It does not install a page
main-world global, call `preventDefault()`, or change scrolling. The observer is
diagnostic overhead, not ordinary platform support. Missing listener verdicts,
cancelled events and nested-scrollport candidates have null attributed frame
latency. Main-frame scroll metadata cannot prove which nested pixels were painted.
`no_viewport_motion_observed` is not inferred cancellation: exhausted scroll
ranges, zero deltas, or unavailable frame evidence can all produce it. Retired,
superseded, unacknowledged and failed inputs also remain explicit. Records are
bounded to 128; requested, unattempted and omitted counts are reported.

The CDP reply and compositor frame may precede the trusted main-thread listener
event. Verdict acquisition therefore polls asynchronously within one absolute
deadline beginning at the CDP reply: three quarters of the action spacing,
clamped to 100–1000 ms. Waiting for the verdict and frame shares that budget;
neither step resets it. Exact sequence/delta matching is required before the
deadline, and late verdicts or frames cannot gain ownership. Acquiring a later
verdict does not replace the original first-frame timestamp. These independent
endpoints are not evidence that listener dispatch preceded the compositor frame.

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
`--native-wheel-browser-only` runs six owned trusted-event fixtures through the
same hidden Chrome launch path, including missing observation and retirement.

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
