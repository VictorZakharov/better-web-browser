# Screen Wake Lock

Breeze implements the `screen` type of the [Screen Wake Lock API](https://w3c.github.io/screen-wake-lock/)
for secure, visible top-level documents. `navigator.wakeLock` is a same-object
property; `request()` resolves to a `WakeLockSentinel` only after browser
admission, and release changes `released` and dispatches a trusted `release`
event. Descendant frames are denied until their effective secure context and
policy delegation can be validated by the browser.

The browser process verifies the committed document, active tab, and visible
window before owning a Windows display/system power request. At most 16 locks
per tab and 64 overall are admitted; all locks are retired on visibility loss,
tab switch, navigation, renderer exit, tab close, or window teardown. Operating
system acquisition is advisory and its success is not exposed to pages. Tests
use a fake provider rather than changing the user's display power state. This
slice uses the existing `windows-sys` dependency and no copied upstream code.

The response `Permissions-Policy: screen-wake-lock=()` header is not yet
retained at browser admission, so top-level policy denial is not enforced.
The recommended active-lock indicator and user revocation control are also
not implemented. There is no worker exposure or support for other lock types.
