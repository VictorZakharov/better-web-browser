//! Real, browser-process Windows device presence. No IDs or names leave this module.

use super::{MediaDeviceProvider, Presence, PresenceResult};
use crate::windows_app::WinRtApartment;
use better_web_browser::renderer_protocol::MediaDeviceError;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, SyncSender};
use windows::Devices::Enumeration::{DeviceClass, DeviceInformation};

const MAX_QUEUED_JOBS: usize = 64;

struct Job {
    done: Box<dyn FnOnce(PresenceResult) + Send>,
}

/// A single MTA worker waits for WinRT operations off the browser UI thread.
/// Completion handlers installed by `join` only signal the waiter; collection
/// getters run on this initialized worker, never on an arbitrary callback thread.
pub(super) struct WinRtMediaDeviceProvider {
    worker: Mutex<Option<SyncSender<Job>>>,
}

impl Default for WinRtMediaDeviceProvider {
    fn default() -> Self {
        Self {
            worker: Mutex::new(None),
        }
    }
}

impl MediaDeviceProvider for WinRtMediaDeviceProvider {
    fn enumerate(
        &self,
        done: Box<dyn FnOnce(PresenceResult) + Send>,
    ) -> Result<(), MediaDeviceError> {
        let mut worker = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if worker.is_none() {
            let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_JOBS);
            std::thread::Builder::new()
                .name("breeze-media-device-enumeration".into())
                .spawn(move || work(receiver))
                .map_err(|_| MediaDeviceError::NotReadable)?;
            *worker = Some(sender);
        }
        worker
            .as_ref()
            .expect("worker sender installed")
            .try_send(Job { done })
            .map_err(|_| MediaDeviceError::NotReadable)
    }
}

fn work(receiver: Receiver<Job>) {
    let apartment = WinRtApartment::initialize_worker();
    for job in receiver {
        let result = if apartment.is_ok() {
            enumerate_presence()
        } else {
            Err(MediaDeviceError::NotReadable)
        };
        (job.done)(result);
    }
}

fn enumerate_presence() -> PresenceResult {
    // The two snapshots are started together; neither asks for media capture.
    let microphones =
        DeviceInformation::FindAllAsyncDeviceClass(DeviceClass::AudioCapture).map_err(map_error)?;
    let cameras =
        DeviceInformation::FindAllAsyncDeviceClass(DeviceClass::VideoCapture).map_err(map_error)?;
    Ok(Presence {
        microphone: microphones
            .join()
            .map_err(map_error)?
            .Size()
            .map_err(map_error)?
            > 0,
        camera: cameras
            .join()
            .map_err(map_error)?
            .Size()
            .map_err(map_error)?
            > 0,
    })
}

fn map_error(error: windows::core::Error) -> MediaDeviceError {
    match error.code().0 as u32 {
        0x8007_0005 => MediaDeviceError::NotAllowed,
        _ => MediaDeviceError::NotReadable,
    }
}
