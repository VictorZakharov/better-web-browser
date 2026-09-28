//! Ambient light in lux, quantized before it can leave the browser process.

use super::{NativeSensors, Sample, SensorError, SensorReading, current, rounded};

const LIGHT_ROUNDING_LUX: f64 = 50.0;

impl NativeSensors {
    pub(super) fn illuminance(&mut self) -> Result<Option<Sample>, SensorError> {
        let Some(reading) = current(self.light()?.GetCurrentReading())? else {
            return Ok(None);
        };
        let stamp = reading
            .Timestamp()
            .map_err(|_| SensorError::NotReadable)?
            .UniversalTime;
        let raw = f64::from(
            reading
                .IlluminanceInLux()
                .map_err(|_| SensorError::NotReadable)?,
        );
        let illuminance = quantize_light(raw)?;
        Ok(Some(Sample {
            stamp,
            reading: SensorReading::Illuminance {
                illuminance,
                timestamp_ms: self.clock.elapsed().as_secs_f64() * 1000.0,
            },
            // This raw value remains private to the browser worker's threshold
            // check. Only the 50-lux multiple is serialized to the renderer.
            raw_light_lux: Some(raw),
        }))
    }
}

fn quantize_light(raw_lux: f64) -> Result<f64, SensorError> {
    if !raw_lux.is_finite() || raw_lux < 0.0 {
        return Err(SensorError::NotReadable);
    }
    // W3C Ambient Light Sensor §3.1 and §6.3 require a rounding multiple of
    // at least 50 lux. The worker separately requires a 25-lux raw delta and
    // a changed quantized value before publishing a reading.
    Ok(rounded(raw_lux, LIGHT_ROUNDING_LUX))
}

#[cfg(test)]
mod tests {
    use super::quantize_light;

    #[test]
    fn light_is_quantized_to_fifty_lux_before_ipc() {
        assert_eq!(quantize_light(0.0).unwrap(), 0.0);
        assert_eq!(quantize_light(24.0).unwrap(), 0.0);
        assert_eq!(quantize_light(26.0).unwrap(), 50.0);
        assert_eq!(quantize_light(74.0).unwrap(), 50.0);
        assert_eq!(quantize_light(76.0).unwrap(), 100.0);
        assert!(quantize_light(-1.0).is_err());
        assert!(quantize_light(f64::NAN).is_err());
    }
}
