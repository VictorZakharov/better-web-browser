# Renderer event delivery

## Contract

Renderer-to-browser events use the existing bounded queue and retained/coalesced
presentation, runtime and video updates. The queue optionally notifies its consumer
when work becomes available. Registering a notifier also wakes for existing queued
work, including startup events that preceded registration.

The notifier is latched while the consumer handles a batch. Callback invocation is
outside the queue lock. Finishing a drain re-arms it under that same lock only if
the queue is empty. A concurrent arrival therefore either leaves a continuation
batch or triggers a new wake; it cannot fall between checking and re-arming.

The Win32 adapter posts a pointer-free message containing a stable tab identity and
renderer session identity. It resolves the tab's current window when posting and
forwards a queued message if the tab moved. Closed tabs and replaced sessions ignore
stale wakes. Queue closure releases the callback; an already in-flight wake carries
no object pointer that could outlive its owner.

Each shell drain takes at most 32 events. A remaining batch holds the notification
latch and continues through the existing low-priority 16 ms monitor timer, rather
than an unlimited posted-message continuation chain. Terminal snapshots do not
discard events still awaiting a subsequent batch. The existing 250 ms idle monitor
remains for health/deadlines and failed native message posting. Active playback,
input and runtime polling policies otherwise remain unchanged.

This follows the native [PostMessage contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew)
and [message-queue priorities](https://learn.microsoft.com/en-us/windows/win32/winmsg/about-messages-and-message-queues):
posting is asynchronous and can fail, and a flood of posted messages can delay input,
painting and timers. No shorter idle polling loop, new helper thread, queue-capacity
increase, renderer privilege change or site-specific policy is required.

## Validation

Queue regressions cover burst coalescing, partial drains, arrivals during handling,
registration after enqueue, callbacks outside the queue lock, concurrent re-arming,
failed-message fallback, teardown and lossless Fetch notifications. An isolated
renderer test waits on notifications for successive documents and process failure,
without periodically polling to discover events. Existing hidden browser reload,
navigation, input, streaming Fetch, media and fullscreen tests exercise the adapter.

Faster delivery exposed a semantic-delta coalescing defect during immediate fullscreen
entry/exit: a stable DOM node removed from the accessibility tree and explicitly added
back before delivery was rejected as an identity violation. Coalescing now cancels the
temporary removal and treats that node as an update relative to the consumer's tree.
An undeclared reintroduction is still rejected. Regression tests check the positive and
negative cases and exhaust 1,024 visibility sequences for two independent nodes.
The previously failing hidden fullscreen-exit test passed 15 consecutive local runs
after this correction; the timeout and assertions were not relaxed.

For performance, use the [owned async-script fixture](loading-standards.md#reproducing-the-owned-comparison).
It withholds an early script for two seconds while a later shared source responds
after 100 ms. The script-relative first-fast milestone and the navigation-relative
filmstrip are separate measurements. A 500 ms filmstrip records observation bounds,
not exact paint timestamps. Neither this fixture nor the browser's initial
`page_ready_ms` is a claim about whole-page YouTube readiness.

## Release evidence — 2026-09-09

Five serial rounds, each running the merged baseline (`3871cc2`), this change, then
Chromium `152.0.7977.83`, with fresh profiles and no concurrent builds or test suites.
The same local server, 1.25 device scale, 2,800 ms settling and 500 ms filmstrip
interval were used. Breeze's 1,520 x 1,000 window reported a 1,505.6 x 828 CSS-pixel
viewport; Chromium used the rounded 1,506 x 828 viewport. Neither run requests audible
output or visible windows. Diagnostic selectors are disabled for Breeze timings
because they enable profiling; Chromium collects `#fast` and `#result` after settling.

| Measurement | Merged baseline | Event notifications | Chromium |
| --- | --- | --- | --- |
| First fast script: median (range) | 340 ms (321–346) | 112 ms (103–116) | 116 ms (103–120) |
| Raw first-fast samples, in round order | 346, 346, 340, 324, 321 ms | 112, 103, 116, 112, 113 ms | 120, 111, 103, 118, 116 ms |
| First filmstrip sample showing fast content, all five runs | 1.0 s | 0.5 s | 0.5 s |
| Fast element executions / failed-element error events | 2 / 2 | 2 / 2 | 2 / 2 |

The measured first-fast delay is 67% lower than the merged baseline. The new range
overlaps Chromium's: this controlled milestone is now comparable, not evidence that
Breeze is generally faster or that whole-page startup meets a 10% Chrome margin.
No CPU or memory improvement is claimed. Async HTML semantics and the
[remaining standards work](loading-standards.md#remaining-standards-slices) are unchanged.

Both early and settled captures were inspected: the fast box appears while the slow
box is still pending, then both settle correctly. Raw reports and filmstrips remain
in ignored `target/event-notifications/{before,after,reference}-9` through `-13`.
Run the same fixture commands in the linked reproduction guide to collect fresh
evidence; these local capture artifacts are not part of the source distribution.
