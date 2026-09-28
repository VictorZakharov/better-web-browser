//! Distinct WinRT standard, linear, and gravity acceleration sources.

use super::{
    NativeSensors, Sample, SensorError, SensorReading, classify_default_error, current, rounded,
};
use windows::Devices::Sensors::{Accelerometer, AccelerometerReadingType};

const STANDARD_GRAVITY: f64 = 9.80665;

#[derive(Clone, Copy)]
pub(super) enum AccelerationSource {
    Standard,
    Linear,
    Gravity,
}

impl NativeSensors {
    pub(super) fn accelerometer(&mut self) -> Result<Accelerometer, SensorError> {
        self.initialized()?;
        self.accelerometer
            .get_or_insert_with(|| Accelerometer::GetDefault().map_err(classify_default_error))
            .clone()
    }

    pub(super) fn linear_accelerometer(&mut self) -> Result<Accelerometer, SensorError> {
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

    pub(super) fn gravity_accelerometer(&mut self) -> Result<Accelerometer, SensorError> {
        self.initialized()?;
        self.gravity_accelerometer
            .get_or_insert_with(|| {
                Accelerometer::GetDefaultWithAccelerometerReadingType(
                    AccelerometerReadingType::Gravity,
                )
                .map_err(classify_default_error)
            })
            .clone()
    }

    pub(super) fn acceleration(
        &mut self,
        source: AccelerationSource,
    ) -> Result<Option<(i64, [f64; 3])>, SensorError> {
        let device = match source {
            AccelerationSource::Standard => self.accelerometer()?,
            AccelerationSource::Linear => self.linear_accelerometer()?,
            AccelerationSource::Gravity => self.gravity_accelerometer()?,
        };
        let Some(reading) = current(device.GetCurrentReading())? else {
            return Ok(None);
        };
        let stamp = reading
            .Timestamp()
            .map_err(|_| SensorError::NotReadable)?
            .UniversalTime;
        let values = [
            reading.AccelerationX(),
            reading.AccelerationY(),
            reading.AccelerationZ(),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SensorError::NotReadable)?;
        let [x, y, z] = values.try_into().expect("three acceleration components");
        Ok(Some((stamp, [x, y, z].map(g_to_meters_per_second_squared))))
    }

    pub(super) fn acceleration_sample(
        &mut self,
        source: AccelerationSource,
    ) -> Result<Option<Sample>, SensorError> {
        let sample = self.acceleration(source)?;
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
}

fn g_to_meters_per_second_squared(value: f64) -> f64 {
    rounded(value * STANDARD_GRAVITY, 0.1)
}

#[cfg(test)]
mod tests {
    use super::g_to_meters_per_second_squared;

    #[test]
    fn standard_linear_and_gravity_use_the_same_physical_unit_conversion() {
        assert_eq!(g_to_meters_per_second_squared(1.0), 9.8);
        assert_eq!(g_to_meters_per_second_squared(-1.0), -9.8);
        assert_eq!(g_to_meters_per_second_squared(0.0), 0.0);
        assert_eq!(g_to_meters_per_second_squared(-0.0125), -0.1);
    }
}
