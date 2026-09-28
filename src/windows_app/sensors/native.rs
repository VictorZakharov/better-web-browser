//! WinRT adapter. No physical sensor is opened until a permissioned Start reaches this worker.

mod light;
mod orientation;

use super::worker::{self, Sample, SensorProvider};
use better_web_browser::renderer_protocol::{SensorError, SensorKind, SensorReading};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::windows_app::winrt_apartment::WinRtApartment;
use windows::Devices::Sensors::{
    Accelerometer, AccelerometerReadingType, Gyrometer, LightSensor, Magnetometer,
    OrientationSensor,
};
use windows::core::Error as WinError;

use super::TabId;

const STANDARD_GRAVITY: f64 = 9.80665;
const REPORT_INTERVAL_MS: u32 = 20;

pub(super) fn run_native_worker(
    incoming: Receiver<worker::Command>,
    retired: Arc<Mutex<HashMap<TabId, (u64, u64)>>>,
    visible_tab: Arc<AtomicU64>,
    stopping: Arc<AtomicBool>,
) {
    // RoInitialize does not touch physical hardware. Tests use a fake provider instead.
    let apartment = WinRtApartment::initialize_worker().map_err(|_| SensorError::NotReadable);
    worker::run(
        incoming,
        retired,
        visible_tab,
        stopping,
        NativeSensors::new(apartment),
    );
}

struct NativeSensors {
    accelerometer: Option<Result<Accelerometer, SensorError>>,
    linear_accelerometer: Option<Result<Accelerometer, SensorError>>,
    gyrometer: Option<Result<Gyrometer, SensorError>>,
    orientation: Option<Result<OrientationSensor, SensorError>>,
    absolute_orientation: Option<Result<OrientationSensor, SensorError>>,
    magnetometer: Option<Result<Magnetometer, SensorError>>,
    light: Option<Result<LightSensor, SensorError>>,
    clock: Instant,
    // Drop after all WinRT objects have released their references.
    _apartment: Result<WinRtApartment, SensorError>,
}

impl NativeSensors {
    fn new(apartment: Result<WinRtApartment, SensorError>) -> Self {
        Self {
            accelerometer: None,
            linear_accelerometer: None,
            gyrometer: None,
            orientation: None,
            absolute_orientation: None,
            magnetometer: None,
            light: None,
            clock: Instant::now(),
            _apartment: apartment,
        }
    }

    fn initialized(&self) -> Result<(), SensorError> {
        self._apartment.as_ref().map(|_| ()).map_err(|error| *error)
    }

    fn accelerometer(&mut self) -> Result<Accelerometer, SensorError> {
        self.initialized()?;
        self.accelerometer
            .get_or_insert_with(|| Accelerometer::GetDefault().map_err(classify_default_error))
            .clone()
    }

    fn linear_accelerometer(&mut self) -> Result<Accelerometer, SensorError> {
        self.initialized()?;
        self.linear_accelerometer
            .get_or_insert_with(|| {
                Accelerometer::GetDefaultWithAccelerometerReadingType(
                    AccelerometerReadingType::Linear,
                )
                .map_err(classify_default_error)
            })
            .clone()
    }

    fn gyrometer(&mut self) -> Result<Gyrometer, SensorError> {
        self.initialized()?;
        self.gyrometer
            .get_or_insert_with(|| Gyrometer::GetDefault().map_err(classify_default_error))
            .clone()
    }

    fn orientation(&mut self) -> Result<OrientationSensor, SensorError> {
        self.initialized()?;
        self.orientation
            .get_or_insert_with(|| {
                OrientationSensor::GetDefaultForRelativeReadings().map_err(classify_default_error)
            })
            .clone()
    }

    fn absolute_orientation(&mut self) -> Result<OrientationSensor, SensorError> {
        self.initialized()?;
        self.absolute_orientation
            .get_or_insert_with(|| OrientationSensor::GetDefault().map_err(classify_default_error))
            .clone()
    }

    fn magnetometer(&mut self) -> Result<Magnetometer, SensorError> {
        self.initialized()?;
        self.magnetometer
            .get_or_insert_with(|| Magnetometer::GetDefault().map_err(classify_default_error))
            .clone()
    }

    fn light(&mut self) -> Result<LightSensor, SensorError> {
        self.initialized()?;
        self.light
            .get_or_insert_with(|| LightSensor::GetDefault().map_err(classify_default_error))
            .clone()
    }

    fn acceleration(&mut self, linear: bool) -> Result<Option<(i64, [f64; 3])>, SensorError> {
        let device = if linear {
            self.linear_accelerometer()?
        } else {
            self.accelerometer()?
        };
        let Some(reading) = current(device.GetCurrentReading())? else {
            return Ok(None);
        };
        let stamp = reading
            .Timestamp()
            .map_err(|_| SensorError::NotReadable)?
            .UniversalTime;
        let values = [
            reading
                .AccelerationX()
                .map_err(|_| SensorError::NotReadable)?,
            reading
                .AccelerationY()
                .map_err(|_| SensorError::NotReadable)?,
            reading
                .AccelerationZ()
                .map_err(|_| SensorError::NotReadable)?,
        ];
        Ok(Some((
            stamp,
            values.map(|g| rounded(g * STANDARD_GRAVITY, 0.1)),
        )))
    }

    fn angular_velocity(&mut self) -> Result<Option<(i64, [f64; 3])>, SensorError> {
        let Some(reading) = current(self.gyrometer()?.GetCurrentReading())? else {
            return Ok(None);
        };
        let stamp = reading
            .Timestamp()
            .map_err(|_| SensorError::NotReadable)?
            .UniversalTime;
        let degrees = [
            reading
                .AngularVelocityX()
                .map_err(|_| SensorError::NotReadable)?,
            reading
                .AngularVelocityY()
                .map_err(|_| SensorError::NotReadable)?,
            reading
                .AngularVelocityZ()
                .map_err(|_| SensorError::NotReadable)?,
        ];
        Ok(Some((stamp, degrees.map(|value| rounded(value, 0.1)))))
    }

    fn magnetic_field(&mut self) -> Result<Option<(i64, [f64; 3])>, SensorError> {
        let Some(reading) = current(self.magnetometer()?.GetCurrentReading())? else {
            return Ok(None);
        };
        let stamp = reading
            .Timestamp()
            .map_err(|_| SensorError::NotReadable)?
            .UniversalTime;
        // WinRT and W3C Magnetometer both report microteslas in device axes.
        let values = [
            reading.MagneticFieldX(),
            reading.MagneticFieldY(),
            reading.MagneticFieldZ(),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SensorError::NotReadable)?;
        let [x, y, z] = values.try_into().expect("three magnetic components");
        Ok(Some((
            stamp,
            [x, y, z].map(|value| rounded(f64::from(value), 0.1)),
        )))
    }
}

impl SensorProvider for NativeSensors {
    fn supported(&mut self, kind: SensorKind) -> Result<bool, SensorError> {
        let device = match kind {
            SensorKind::Orientation => self.orientation().map(|_| ()),
            SensorKind::Motion => {
                let required = self.accelerometer().map(|_| ());
                // Motion's linear acceleration and rotation rate remain nullable when absent.
                let _ = self.linear_accelerometer();
                let _ = self.gyrometer();
                required
            }
            SensorKind::Accelerometer => self.accelerometer().map(|_| ()),
            SensorKind::Gyroscope => self.gyrometer().map(|_| ()),
            SensorKind::Magnetometer => self.magnetometer().map(|_| ()),
            SensorKind::AbsoluteOrientation => self.absolute_orientation().map(|_| ()),
            SensorKind::RelativeOrientation => self.orientation().map(|_| ()),
            SensorKind::AmbientLight => self.light().map(|_| ()),
        };
        match device {
            Ok(()) => Ok(true),
            Err(SensorError::NotSupported) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn set_active(&mut self, active: bool) {
        let interval = |minimum: windows::core::Result<u32>| {
            if active {
                minimum
                    .unwrap_or(REPORT_INTERVAL_MS)
                    .max(REPORT_INTERVAL_MS)
            } else {
                0
            }
        };
        for cached in [&self.accelerometer, &self.linear_accelerometer] {
            if let Some(Ok(device)) = cached {
                let _ = device.SetReportInterval(interval(device.MinimumReportInterval()));
            }
        }
        if let Some(Ok(device)) = &self.gyrometer {
            let _ = device.SetReportInterval(interval(device.MinimumReportInterval()));
        }
        if let Some(Ok(device)) = &self.orientation {
            let _ = device.SetReportInterval(interval(device.MinimumReportInterval()));
        }
        if let Some(Ok(device)) = &self.absolute_orientation {
            let _ = device.SetReportInterval(interval(device.MinimumReportInterval()));
        }
        if let Some(Ok(device)) = &self.magnetometer {
            let _ = device.SetReportInterval(interval(device.MinimumReportInterval()));
        }
        if let Some(Ok(device)) = &self.light {
            let _ = device.SetReportInterval(interval(device.MinimumReportInterval()));
        }
    }

    fn sample(
        &mut self,
        kind: SensorKind,
        interval_ms: f64,
    ) -> Result<Option<Sample>, SensorError> {
        match kind {
            SensorKind::Orientation => self.orientation_angles(),
            SensorKind::AbsoluteOrientation => self.absolute_quaternion(),
            SensorKind::RelativeOrientation => self.relative_quaternion(),
            SensorKind::AmbientLight => self.illuminance(),
            SensorKind::Accelerometer => {
                let sample = self.acceleration(false)?;
                Ok(sample.map(|(stamp, [x, y, z])| Sample {
                    stamp,
                    reading: SensorReading::ThreeAxis {
                        x,
                        y,
                        z,
                        timestamp_ms: self.clock.elapsed().as_secs_f64() * 1000.0,
                    },
                    raw_light_lux: None,
                }))
            }
            SensorKind::Gyroscope => {
                let sample = self.angular_velocity()?;
                Ok(sample.map(|(stamp, degrees)| {
                    let [x, y, z] = degrees.map(f64::to_radians);
                    Sample {
                        stamp,
                        reading: SensorReading::ThreeAxis {
                            x,
                            y,
                            z,
                            timestamp_ms: self.clock.elapsed().as_secs_f64() * 1000.0,
                        },
                        raw_light_lux: None,
                    }
                }))
            }
            SensorKind::Magnetometer => {
                let sample = self.magnetic_field()?;
                Ok(sample.map(|(stamp, [x, y, z])| Sample {
                    stamp,
                    reading: SensorReading::ThreeAxis {
                        x,
                        y,
                        z,
                        timestamp_ms: self.clock.elapsed().as_secs_f64() * 1000.0,
                    },
                    raw_light_lux: None,
                }))
            }
            SensorKind::Motion => {
                let Some((stamp, gravity)) = self.acceleration(false)? else {
                    return Ok(None);
                };
                let linear = self.linear_accelerometer().ok().and_then(|_| {
                    self.acceleration(true)
                        .ok()
                        .flatten()
                        .map(|(_, values)| values)
                });
                let rotation_sample = self
                    .gyrometer()
                    .ok()
                    .and_then(|_| self.angular_velocity().ok().flatten());
                let rotation = rotation_sample.as_ref().map(|(_, values)| *values);
                let stamp =
                    rotation_sample.map_or(stamp, |(rotation_stamp, _)| stamp.max(rotation_stamp));
                Ok(Some(Sample {
                    stamp,
                    reading: SensorReading::Motion {
                        acceleration: linear,
                        acceleration_including_gravity: Some(gravity),
                        rotation_rate: rotation,
                        interval_ms,
                    },
                    raw_light_lux: None,
                }))
            }
        }
    }
}

fn classify_default_error(error: WinError) -> SensorError {
    if error.code() == WinError::empty().code() {
        SensorError::NotSupported
    } else {
        SensorError::NotReadable
    }
}

fn current<T>(result: windows::core::Result<T>) -> Result<Option<T>, SensorError> {
    match result {
        Ok(reading) => Ok(Some(reading)),
        Err(error) if error.code() == WinError::empty().code() => Ok(None),
        Err(_) => Err(SensorError::NotReadable),
    }
}

fn rounded(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

#[cfg(test)]
mod tests {
    use super::{STANDARD_GRAVITY, rounded};

    #[test]
    fn acceleration_uses_meters_per_second_squared_and_privacy_quantization() {
        assert_eq!(rounded(STANDARD_GRAVITY, 0.1), 9.8);
        assert_eq!(rounded(-0.123, 0.1), -0.1);
    }
}
