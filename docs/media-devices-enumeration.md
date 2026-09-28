# Media-device enumeration before capture

Breeze implements the secure-window `navigator.mediaDevices.enumerateDevices()`
method as a browser-owned, asynchronous presence query. On Windows, the browser
queries the operating system for audio-capture and video-capture devices. A page
receives at most one `audioinput` and one `videoinput` entry. Before a capture
grant, each entry has empty `deviceId`, `label`, and `groupId` strings, and
`InputDeviceInfo.getCapabilities()` returns an empty object. The renderer never
receives native device identifiers or names.

Enumeration waits until its owning document is fully active and visible. Requests
from an unsupported descendant browsing context or a non-trustworthy origin are
denied. The browser resolves each request client against the active document;
navigation, renderer replacement, and tab closure retire outstanding requests.
An operating-system enumeration failure rejects with `NotReadableError`.

This is deliberately a **partial** Media Capture interface. `getUserMedia()`,
capture permission grants, persistent device identity, `devicechange`, and
audio-output enumeration are not exposed without their corresponding behavior.
Successful enumeration does not authorize capture.

The behavior follows [Media Capture and Streams, §9.2 and §9.4](https://www.w3.org/TR/mediacapture-streams/#enumerating-devices)
and uses [Windows `DeviceInformation.FindAllAsync`](https://learn.microsoft.com/en-us/uwp/api/windows.devices.enumeration.deviceinformation.findallasync)
for the underlying presence check. Unit and hidden-renderer tests inject fake
results; automated tests never ask the operating system to enumerate devices.
