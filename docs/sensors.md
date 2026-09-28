# Motion and orientation sensors

Breeze's Windows sensor slice connects page JavaScript to physical WinRT devices.
It does not synthesize readings to satisfy an API probe. A sensor without suitable
hardware reports a `SensorErrorEvent` with `NotSupportedError` or
`NotReadableError` after `start()`, and automated tests use a fake provider
instead of touching the user's hardware.

## Supported surface

- `DeviceOrientationEvent` and `DeviceMotionEvent` deliver trusted window events
  only while a listener exists. `requestPermission()` requires transient user
  activation. The orientation event uses the relative WinRT orientation sensor
  and reports `absolute: false`; `requestPermission(true)` is denied because
  this legacy event does not currently have an absolute source.
- Generic `Sensor`, `Accelerometer`, `Gyroscope`, `Magnetometer`,
  `OrientationSensor`, `RelativeOrientationSensor`,
  `AbsoluteOrientationSensor`, and `AmbientLightSensor` provide
  `start()`/`stop()`, state, readings, and errors. Accelerometer values are
  m/s², gyroscope values are radians/s, magnetometer values are µT, relative
  and absolute orientations are normalized `[x, y, z, w]` quaternions, and
  ambient light is lux. The legacy motion event's rotation rates remain
  degrees/s, as specified for that event.
- `OrientationSensor.populateMatrix()` writes a 4×4 column-major rotation matrix
  into a `Float32Array`, `Float64Array`, or available `DOMMatrix`. The
  `referenceFrame: 'device'` option is supported; `screen` raises
  `NotSupportedError` until display rotation can be applied correctly.

WinRT `OrientationSensor::GetDefaultForRelativeReadings()` supplies the legacy
event and Generic relative orientation, while `GetDefault()` supplies the
Earth-referenced quaternion. The browser never upgrades the relative reading
to an absolute claim. Windows
sensor axes are in the hardware's natural device orientation, not the current
screen orientation. The adapter converts accelerometer *g* values using
9.80665 m/s² per *g*, and converts gyroscope degrees/s to radians/s only for
the Generic Sensor interface. The worker quantizes vector values to 0.1 in
their respective units and caps sampling at 50 Hz. A Generic Sensor's
`timestamp` currently uses the renderer delivery time from `performance.now()`;
it can lag physical acquisition across the browser/renderer IPC boundary.

WinRT `LightSensor::GetDefault()` supplies ambient lux when the computer has
an integrated sensor. Following [Ambient Light Sensor §3.1 and §6](https://www.w3.org/TR/ambient-light/#reducing-sensor-readings-accuracy),
the browser rounds lux to 50-lux multiples and publishes a new reading only
when the raw value differs from the last published reading by at least 25 lux
**and** the quantized bucket changes. Raw lux stays in the browser worker and
never crosses IPC. The default ambient sampling target is 10 Hz (also subject
to hardware minimum interval and the global 50 Hz cap).

## Authority and lifecycle

The browser process—not the renderer—owns per-origin, session-only permission,
hardware access, and the visible-tab gate. Only a top-level secure origin or
numeric loopback HTTP origin may ask. Opaque, `file:`, and descendant frames
cannot borrow a top-level permission; Permissions Policy delegation is not yet
implemented. Legacy permission prompts require a foreground visible tab and
transient activation. A Generic Sensor `start()` can prompt in a foreground
visible tab. Accelerometer, gyroscope, and magnetometer grants are independent;
relative orientation requires the accelerometer and gyroscope, and absolute
orientation requires all three. Ambient light has its own
`ambient-light-sensor` grant. A declined combined prompt does not revoke a
separately granted capability. Permission prompts do not appear in headless
benchmark modes.

The worker admits at most 32 streams, accepts commands through a bounded
64-entry queue, and sends readings through a bounded renderer mailbox. It
stops sampling hidden tabs and retires streams when the document, renderer,
tab, or window exits. Sensor objects are initialized lazily after permission,
and shutdown does not block on a faulty device driver. Source WinRT timestamps
deduplicate repeated current readings.

The current slice does not include `DeviceOrientationEventAbsolute`,
`UncalibratedMagnetometer`, screen-reference
frame transforms, background sampling, persistent grants, or Permissions
Policy-based iframe delegation. It does not claim full Generic Sensor or
Device Orientation conformance.

References: [W3C Device Orientation and Motion](https://www.w3.org/TR/orientation-event/),
[Generic Sensor](https://www.w3.org/TR/generic-sensor/),
[Accelerometer](https://www.w3.org/TR/accelerometer/),
[Magnetometer](https://www.w3.org/TR/magnetometer/),
[Orientation Sensor](https://www.w3.org/TR/orientation-sensor/),
[Ambient Light Sensor](https://www.w3.org/TR/ambient-light/),
[Microsoft sensor orientation](https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/sensor-orientation),
[WinRT Magnetometer](https://learn.microsoft.com/en-us/uwp/api/windows.devices.sensors.magnetometer), and
[WinRT OrientationSensor](https://learn.microsoft.com/en-us/uwp/api/windows.devices.sensors.orientationsensor), and
[WinRT LightSensor](https://learn.microsoft.com/en-us/uwp/api/windows.devices.sensors.lightsensor).
