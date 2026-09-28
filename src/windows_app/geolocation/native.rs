//! Real Windows location source. No tests instantiate this provider.
//!
//! RequestAccessAsync must be started by the foreground UI thread; dispatch.rs
//! enforces that before calling this adapter.
use super::{GeoFailure, GeoHandle, GeoProvider, GeoResult};
use crate::windows_app::WinRtApartment;
use better_web_browser::renderer_protocol::GeolocationPosition;
use std::sync::{Arc, Mutex};
use windows::Devices::Geolocation::{
    GeolocationAccessStatus, Geolocator, Geoposition, PositionAccuracy, PositionChangedEventArgs,
    PositionStatus, StatusChangedEventArgs,
};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows_future::AsyncOperationCompletedHandler;

pub(super) struct WinRtGeoProvider;

impl GeoProvider for WinRtGeoProvider {
    fn request_access(
        &self,
        done: Box<dyn FnOnce(Result<(), GeoFailure>) + Send>,
    ) -> Result<(), GeoFailure> {
        let operation = Geolocator::RequestAccessAsync().map_err(map_error)?;
        let done = Mutex::new(Some(done));
        let handler =
            AsyncOperationCompletedHandler::<GeolocationAccessStatus>::new(move |sender, _| {
                // A WinRT completion may run on a different or already-initialized
                // thread; initialize before touching GetResults.
                let result = match WinRtApartment::initialize_callback() {
                    Ok(_apartment) => sender
                        .ok()
                        .and_then(|operation| operation.GetResults())
                        .map_err(map_error)
                        .and_then(|status| match status {
                            GeolocationAccessStatus::Allowed => Ok(()),
                            _ => Err(GeoFailure::PermissionDenied),
                        }),
                    Err(error) => Err(map_error(error)),
                };
                if let Some(done) = done
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                {
                    done(result);
                }
                Ok(())
            });
        operation.SetCompleted(&handler).map_err(map_error)
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
        let mut handle = WinRtGeoHandle {
            locator,
            position_token: None,
            status_token: None,
        };
        if watch {
            let receiver = Arc::clone(&deliver);
            let handler =
                TypedEventHandler::<Geolocator, PositionChangedEventArgs>::new(move |_, args| {
                    let result = match WinRtApartment::initialize_callback() {
                        Ok(_apartment) => args
                            .as_ref()
                            .ok_or(GeoFailure::PositionUnavailable)
                            .and_then(|args| args.Position().map_err(map_error))
                            .and_then(position_from_winrt),
                        Err(error) => Err(map_error(error)),
                    };
                    receiver(result);
                    Ok(())
                });
            handle.position_token = Some(
                handle
                    .locator
                    .PositionChanged(&handler)
                    .map_err(map_error)?,
            );

            let receiver = Arc::clone(&deliver);
            let handler =
                TypedEventHandler::<Geolocator, StatusChangedEventArgs>::new(move |_, args| {
                    // Windows sends permission and availability changes on a callback thread.
                    // Only the browser UI thread will retire the watch after this delivery.
                    let failure = match WinRtApartment::initialize_callback() {
                        Ok(_apartment) => args
                            .as_ref()
                            .ok_or(GeoFailure::PositionUnavailable)
                            .and_then(|args| args.Status().map_err(map_error))
                            .map(status_failure)
                            .unwrap_or(Some(GeoFailure::PositionUnavailable)),
                        Err(_) => Some(GeoFailure::PositionUnavailable),
                    };
                    if let Some(failure) = failure {
                        receiver(Err(failure));
                    }
                    Ok(())
                });
            handle.status_token = Some(handle.locator.StatusChanged(&handler).map_err(map_error)?);
        }
        let age = TimeSpan {
            Duration: millis_to_ticks(maximum_age_millis),
        };
        // WinRT requires a finite timeout; the Web IDL Infinity default is
        // represented by the largest signed TimeSpan. Browser-side deadlines
        // independently enforce precise finite timeouts.
        let timeout = TimeSpan {
            Duration: millis_to_ticks(timeout_millis).max(10_000_000),
        };
        let operation = handle
            .locator
            .GetGeopositionAsyncWithAgeAndTimeout(age, timeout)
            .map_err(map_error)?;
        let handler = AsyncOperationCompletedHandler::<Geoposition>::new(move |sender, _| {
            let result = match WinRtApartment::initialize_callback() {
                Ok(_apartment) => sender
                    .ok()
                    .and_then(|operation| operation.GetResults())
                    .map_err(map_error)
                    .and_then(position_from_winrt),
                Err(error) => Err(map_error(error)),
            };
            deliver(result);
            Ok(())
        });
        operation.SetCompleted(&handler).map_err(map_error)?;
        Ok(Box::new(handle))
    }
}

struct WinRtGeoHandle {
    locator: Geolocator,
    position_token: Option<i64>,
    status_token: Option<i64>,
}

impl GeoHandle for WinRtGeoHandle {}

impl Drop for WinRtGeoHandle {
    fn drop(&mut self) {
        if let Some(token) = self.position_token.take() {
            let _ = self.locator.RemovePositionChanged(token);
        }
        if let Some(token) = self.status_token.take() {
            let _ = self.locator.RemoveStatusChanged(token);
        }
    }
}

fn status_failure(status: PositionStatus) -> Option<GeoFailure> {
    if status == PositionStatus::Disabled {
        Some(GeoFailure::PermissionDenied)
    } else if status == PositionStatus::NoData || status == PositionStatus::NotAvailable {
        Some(GeoFailure::PositionUnavailable)
    } else {
        None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_status_distinguishes_revocation_from_temporary_unavailability() {
        assert_eq!(
            status_failure(PositionStatus::Disabled),
            Some(GeoFailure::PermissionDenied)
        );
        assert_eq!(
            status_failure(PositionStatus::NoData),
            Some(GeoFailure::PositionUnavailable)
        );
        assert_eq!(status_failure(PositionStatus::Ready), None);
    }
}
