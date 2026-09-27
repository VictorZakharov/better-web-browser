# Window Notifications

Breeze implements a bounded, browser-owned subset of the
[Notifications API](https://notifications.spec.whatwg.org/). A document can call
`Notification.requestPermission()`, inspect `Notification.permission`, construct a notification,
call `close()`, and receive `show`, `click`, `close`, or `error` events. This is not a general
implementation of the [Permissions API](https://www.w3.org/TR/permissions/).

The browser resolves every renderer request to its registered document/frame client. Permission is
keyed to that client's effective canonical HTTPS origin, including its port, not to an arbitrary
origin string supplied by JavaScript or recovered from the frame URL. The initial child-frame
sandbox decision still originates in renderer navigation metadata; this is not a fully
security-audited browser-owned frame tree. An active top-level document with a browser-recorded
transient user gesture can trigger a Windows Yes/No permission prompt. Descendant frames cannot
prompt yet because activation tracking is not frame-scoped; an existing grant still applies to a
frame of that origin. The choice is retained only until the browser exits; unknown permission
remains `default`. HTTP and opaque origins are denied. Hidden benchmark/test windows never prompt
or display notifications. The renderer cannot change the decision or access Windows notification
APIs directly.

After a grant, the browser presents a notification through Windows
[`Shell_NotifyIconW`](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw)
with an actual notification-area balloon. Win32 callbacks drive `show`, `click`, and `close`; a
failure to install the shell icon produces `error`. `close()` removes the icon, and navigation, tab
close, renderer replacement, or window destruction retire outstanding icons. Non-empty `tag`
values replace a previous active balloon with the same origin and tag. The browser allows at most
16 active notifications in total; the shell may suppress presentation under operating-system
notification settings.

Permission decisions and required error/close results use a separate bounded broker control lane.
The UI thread never blocks on a stalled renderer, and control-lane overflow explicitly fails that
renderer session rather than leaving a live `requestPermission()` Promise pending. Optional shell
`show` and `click` callbacks remain best-effort when a renderer is not consuming events.

This slice does not implement persistent grants, service-worker or worker notifications,
`ServiceWorkerRegistration.showNotification()`, actions, images/icons/badges, vibration, sound
policy, `requireInteraction`, or notification `data`. Such options are rejected when materially
requested instead of being silently presented as supported. The shell backend truncates visual
title/body text to its native UTF-16 fields; protocol input is bounded to 512 title bytes, 2 KiB
body bytes, and 256 tag bytes. Only HTTPS is treated as eligible in this first slice, so HTTP
loopback's potentially trustworthy origin exception is not yet included. Permission snapshots are
seeded for top-level document startup; a newly created child frame may initially report `default`
until it requests its own browser decision.

Validation is deliberately headless: protocol round trips and bounds, retained-realm Promise/event
behavior, browser permission-map scoping, and hidden AppContainer renderer request/update delivery.
The Windows shell presentation itself requires a consented interactive manual check and is not
exercised by automated tests.
