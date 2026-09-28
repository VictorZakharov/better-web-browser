# Browser-owned Geolocation

Breeze implements the secure-document `navigator.geolocation` entry point with
`getCurrentPosition()`, `watchPosition()`, and `clearWatch()`. It obtains real
positions from Windows `Geolocator`; the renderer never invokes Windows
positioning APIs. Position, coordinate, and error callbacks receive branded
interface objects, including `toJSON()` on successful positions and coordinates.

The browser resolves each renderer request against the active document and
origin. Only a trustworthy top-level document can request location. Descendant
documents fail closed until Permissions Policy is supported; a child frame cannot
borrow the top-level page's grant. A browser-owned confirmation prompt asks once
per origin per browser session, followed by Windows' own location-access decision.
No grant is saved across restarts. The native Windows access call is initiated
only while Breeze is the foreground window; hidden tabs pause acquisition and
watch delivery. Navigation, renderer replacement, tab closure, and `clearWatch()`
retire their native watches and late callbacks.

The API honors `enableHighAccuracy`, `maximumAge`, and `timeout` within bounded
request and event queues. A cached position is keyed by origin and requested
accuracy, and is returned only while it meets the requested maximum age. WinRT
timestamps are converted to Unix milliseconds; optional altitude, heading, and
speed fields stay nullable when Windows does not provide them. Position failures
are reported with the standard permission-denied, position-unavailable, or
timeout codes. A denied Windows location permission is not bypassed or replaced
with IP-based synthetic coordinates.

This is a partial implementation of the [W3C Geolocation Candidate
Recommendation](https://www.w3.org/TR/geolocation/). Browser prompts are
session-scoped, not a complete Permissions API integration. Cross-origin frame
delegation, background acquisition, persistent grants, and emulated positions
are not implemented. Tests inject fake position providers and exercise the
renderer IPC without opening visible windows or requesting the user's actual
location; native OS positioning is compile-checked but not exercised by CI.
