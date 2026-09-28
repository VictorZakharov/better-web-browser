//! Real Windows location source. No tests instantiate this provider.
//!
//! RequestAccessAsync must be started by the foreground UI thread; dispatch.rs
//! enforces that before calling this adapter.
use super::{GeoFailure, GeoHandle, GeoProvider, GeoResult};
use better_web_browser::renderer_protocol::GeolocationPosition;
use std::sync::Arc;
use windows::Devices::Geolocation::{
    GeolocationAccessStatus, Geolocator, Geoposition, PositionAccuracy, PositionChangedEventArgs,
};
use windows::Foundation::{TimeSpan, TypedEventHandler};

pub(super) struct WinRtGeoProvider;

impl GeoProvider for WinRtGeoProvider {
    fn request_access(
        &self,
        done: Box<dyn FnOnce(Result<(), GeoFailure>) + Send>,
    ) -> Result<(), GeoFailure> {
        Geolocator::RequestAccessAsync()
            .map_err(map_error)?
            .when(move |result| {
                done(match result {
                    Ok(GeolocationAccessStatus::Allowed) => Ok(()),
                    Ok(_) => Err(GeoFailure::PermissionDenied),
                    Err(error) => Err(map_error(error)),
                });
            })
            .map_err(map_error)
    }

    fn start(
        &self,
        watch: bool,
        high_accuracy: bool,
        maximum_age_millis: u64,
        timeout_millis: u64,
        deliver: Arc<dyn Fn(GeoResult) + Send + Sync>,
    ) -> Result<Box<dyn GeoHandle>, GeoFailure> {
        let locator = Geolocator::new().map_err(map_error)?;
        locator
            .SetDesiredAccuracy(if high_accuracy {
                PositionAccuracy::High
            } else {
                PositionAccuracy::Default
            })
            .map_err(map_error)?;
        let token = if watch {
            let receiver = Arc::clone(&deliver);
            let handler =
                TypedEventHandler::<Geolocator, PositionChangedEventArgs>::new(move |_, args| {
                    if let Some(args) = args.as_ref() {
                        receiver(
                            args.Position()
                                .map_err(map_error)
                                .and_then(position_from_winrt),
                        );
                    }
                    Ok(())
                });
            Some(locator.PositionChanged(&handler).map_err(map_error)?)
        } else {
            None
        };
        let age = TimeSpan {
            Duration: millis_to_ticks(maximum_age_millis),
        };
        // WinRT requires a finite timeout; the Web IDL Infinity default is
        // represented by the largest signed TimeSpan. Browser-side deadlines
        // independently enforce precise finite timeouts.
        let timeout = TimeSpan {
            Duration: millis_to_ticks(timeout_millis).max(10_000_000),
        };
        let operation = locator
            .GetGeopositionAsyncWithAgeAndTimeout(age, timeout)
            .map_err(map_error)?;
        operation
            .when(move |result| deliver(result.map_err(map_error).and_then(position_from_winrt)))
            .map_err(map_error)?;
        Ok(Box::new(WinRtGeoHandle { locator, token }))
    }
}

struct WinRtGeoHandle {
    locator: Geolocator,
    token: Option<i64>,
}

impl GeoHandle for WinRtGeoHandle {}

impl Drop for WinRtGeoHandle {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            let _ = self.locator.RemovePositionChanged(token);
        }
    }
}

fn millis_to_ticks(millis: u64) -> i64 {
    if millis == u64::MAX {
        i64::MAX
    } else {
        millis.saturating_mul(10_000).min(i64::MAX as u64) as i64
    }
}

fn optional_number(
    value: windows::core::Result<windows::Foundation::IReference<f64>>,
) -> Option<f64> {
    value.ok().and_then(|reference| reference.Value().ok())
}

fn position_from_winrt(position: Geoposition) -> GeoResult {
    let coordinate = position.Coordinate().map_err(map_error)?;
    // WinRT DateTime ticks are 100 ns since 1601; JavaScript uses ms since 1970.
    const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
    let timestamp_millis = coordinate
        .Timestamp()
        .map_err(map_error)?
        .UniversalTime
        .saturating_sub(UNIX_EPOCH_TICKS)
        .max(0) as u64
        / 10_000;
    let result = GeolocationPosition {
        latitude: coordinate.Latitude().map_err(map_error)?,
        longitude: coordinate.Longitude().map_err(map_error)?,
        accuracy: coordinate.Accuracy().map_err(map_error)?,
        altitude: optional_number(coordinate.Altitude()),
        altitude_accuracy: optional_number(coordinate.AltitudeAccuracy()),
        heading: optional_number(coordinate.Heading()).map(|heading| heading.rem_euclid(360.0)),
        speed: optional_number(coordinate.Speed()),
        timestamp_millis,
    };
    result
        .validate()
        .map_err(|_| GeoFailure::PositionUnavailable)?;
    Ok(result)
}

fn map_error(error: windows::core::Error) -> GeoFailure {
    match error.code().0 as u32 {
        0x8007_0005 | 0x8007_0490 => GeoFailure::PermissionDenied,
        0x8007_05b4 => GeoFailure::Timeout,
        _ => GeoFailure::PositionUnavailable,
    }
}
