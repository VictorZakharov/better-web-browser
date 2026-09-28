//! Fused WinRT orientation readings in the device's natural coordinate frame.

use super::{NativeSensors, Sample, SensorError, SensorReading, current, rounded};

impl NativeSensors {
    pub(super) fn orientation_angles(&mut self) -> Result<Option<Sample>, SensorError> {
        let Some(reading) = current(self.orientation()?.GetCurrentReading())? else {
            return Ok(None);
        };
        let stamp = reading
            .Timestamp()
            .map_err(|_| SensorError::NotReadable)?
            .UniversalTime;
        let matrix = reading
            .RotationMatrix()
            .map_err(|_| SensorError::NotReadable)?;
        let elements = [
            matrix.M11(),
            matrix.M12(),
            matrix.M21(),
            matrix.M22(),
            matrix.M31(),
            matrix.M32(),
            matrix.M33(),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SensorError::NotReadable)?;
        let [m11, m12, m21, m22, m31, m32, m33] =
            elements.try_into().expect("seven matrix components");
        let [alpha, beta, gamma] = euler_zxy([m11, m12, m21, m22, m31, m32, m33].map(f64::from));
        Ok(Some(Sample {
            stamp,
            reading: SensorReading::Orientation {
                alpha: Some(alpha),
                beta: Some(beta),
                gamma: Some(gamma),
                absolute: false,
            },
            raw_light_lux: None,
        }))
    }

    pub(super) fn absolute_quaternion(&mut self) -> Result<Option<Sample>, SensorError> {
        self.quaternion(true)
    }

    pub(super) fn relative_quaternion(&mut self) -> Result<Option<Sample>, SensorError> {
        self.quaternion(false)
    }

    fn quaternion(&mut self, absolute: bool) -> Result<Option<Sample>, SensorError> {
        let device = if absolute {
            self.absolute_orientation()?
        } else {
            self.orientation()?
        };
        let Some(reading) = current(device.GetCurrentReading())? else {
            return Ok(None);
        };
        let stamp = reading
            .Timestamp()
            .map_err(|_| SensorError::NotReadable)?
            .UniversalTime;
        let quaternion = reading.Quaternion().map_err(|_| SensorError::NotReadable)?;
        let values = [
            quaternion.X(),
            quaternion.Y(),
            quaternion.Z(),
            quaternion.W(),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SensorError::NotReadable)?;
        let [x, y, z, w] = values.try_into().expect("four quaternion components");
        let [x, y, z, w] = unit_quaternion([x, y, z, w].map(f64::from))?;
        // WinRT's default fused sensor is magnetic-north referenced; its
        // relative variant omits the magnetic reference. The four-vector order
        // is x, y, z, w in both WinRT and W3C Orientation Sensor.
        Ok(Some(Sample {
            stamp,
            reading: SensorReading::Quaternion {
                x,
                y,
                z,
                w,
                timestamp_ms: self.clock.elapsed().as_secs_f64() * 1000.0,
            },
            raw_light_lux: None,
        }))
    }
}

fn unit_quaternion(values: [f64; 4]) -> Result<[f64; 4], SensorError> {
    let length_squared = values.iter().map(|value| value * value).sum::<f64>();
    if !length_squared.is_finite() || length_squared < 1e-12 {
        return Err(SensorError::NotReadable);
    }
    let length = length_squared.sqrt();
    Ok(values.map(|value| value / length))
}

/// W3C Device Orientation uses intrinsic Z-X'-Y'' angles, unlike roll/pitch/yaw.
/// The WinRT matrix is in the device's natural orientation, independent of screen rotation.
/// See https://www.w3.org/TR/orientation-event/#deviceorientation and Microsoft sensor orientation docs.
fn euler_zxy([m11, m12, m21, m22, m31, m32, m33]: [f64; 7]) -> [f64; 3] {
    // asin selects the equivalent canonical beta in [-90, 90] even though the
    // Device Orientation range also allows an alternative [-180, 180] branch.
    let beta = m32.clamp(-1.0, 1.0).asin();
    let (alpha, gamma) = if beta.cos().abs() < 1e-6 {
        (m21.atan2(m11), 0.0)
    } else {
        ((-m12).atan2(m22), (-m31).atan2(m33))
    };
    let mut alpha = rounded(alpha.to_degrees().rem_euclid(360.0), 0.1).rem_euclid(360.0);
    let mut beta = rounded(beta.to_degrees(), 0.1);
    let mut gamma = rounded(gamma.to_degrees(), 0.1);
    if !(-90.0..90.0).contains(&gamma) {
        // Z(alpha) X(beta) Y(gamma) has an equivalent decomposition with
        // gamma in the specification's half-open [-90, 90) interval.
        gamma += if gamma >= 90.0 { -180.0 } else { 180.0 };
        beta = if beta > 0.0 {
            180.0 - beta
        } else {
            -180.0 - beta
        };
        alpha = (alpha + 180.0).rem_euclid(360.0);
    }
    [alpha, beta, gamma]
}

#[cfg(test)]
mod tests {
    use super::{euler_zxy, unit_quaternion};

    #[test]
    fn orientation_matrix_maps_intrinsic_zxy_axes() {
        assert_eq!(
            euler_zxy([1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
            [0.0, 0.0, 0.0]
        );
        assert_eq!(
            euler_zxy([0.0, -1.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
            [90.0, 0.0, 0.0]
        );
        assert_eq!(
            euler_zxy([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]),
            [0.0, 90.0, 0.0]
        );
        let angle = 359.96_f64.to_radians();
        assert_eq!(
            euler_zxy([
                angle.cos(),
                -angle.sin(),
                angle.sin(),
                angle.cos(),
                0.0,
                0.0,
                1.0
            ])[0],
            0.0
        );
        let gamma = 120.0_f64.to_radians();
        assert_eq!(
            euler_zxy([gamma.cos(), 0.0, 0.0, 1.0, -gamma.sin(), 0.0, gamma.cos()]),
            [180.0, -180.0, -60.0]
        );
    }

    #[test]
    fn quaternion_is_unit_length_without_a_zero_or_nonfinite_fallback() {
        assert_eq!(
            unit_quaternion([0.0, 0.0, 0.0, 2.0]).unwrap(),
            [0.0, 0.0, 0.0, 1.0]
        );
        assert!(unit_quaternion([0.0; 4]).is_err());
        assert!(unit_quaternion([f64::NAN, 0.0, 0.0, 1.0]).is_err());
    }
}
