//! Media Foundation device adapter. None of these APIs run during construction or automated
//! tests. `start` is reachable only after the browser has launched a per-grant capture child and
//! completed its private nonce-bound handshake.

use super::child::CaptureProvider;
use crate::capture_protocol::{CaptureDevices, CaptureFailure, CaptureSample};
use std::time::{Duration, Instant};
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_FULL, MFShutdown, MFStartup};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::Error as WinError;

mod device;
mod samples;
use device::{CameraCapture, MicrophoneCapture};

const VIDEO_PERIOD: Duration = Duration::from_millis(33);

#[derive(Default)]
pub(super) struct NativeCapture {
    camera: Option<CameraCapture>,
    microphone: Option<MicrophoneCapture>,
    video_due: Option<Instant>,
    next_audio: bool,
    // Drop after the source readers and source objects have shut down.
    foundation: Option<MediaFoundation>,
    apartment: Option<ComApartment>,
}

impl CaptureProvider for NativeCapture {
    fn start(&mut self, devices: CaptureDevices, _capture_id: u64) -> Result<(), CaptureFailure> {
        let apartment = ComApartment::enter()?;
        let foundation = MediaFoundation::start()?;
        let camera = devices.camera.then(CameraCapture::open).transpose()?;
        let microphone = devices
            .microphone
            .then(MicrophoneCapture::open)
            .transpose()?;
        self.camera = camera;
        self.microphone = microphone;
        self.video_due = self.camera.as_ref().map(|_| Instant::now());
        self.next_audio = self.microphone.is_some();
        self.foundation = Some(foundation);
        self.apartment = Some(apartment);
        Ok(())
    }

    fn next_sample(&mut self, capture_id: u64) -> Result<Option<CaptureSample>, CaptureFailure> {
        // Separate Source Readers are used for camera and microphone. MF timestamps remain on
        // their source timebases; later MediaStream work must synchronize rather than assuming
        // they share a single clock.
        if self.next_audio {
            self.next_audio = false;
            if let Some(microphone) = &mut self.microphone
                && let Some(sample) = microphone.next_sample(capture_id)?
            {
                return Ok(Some(sample));
            }
        }
        if self.video_due.is_some_and(|due| Instant::now() >= due) {
            self.video_due = Some(Instant::now() + VIDEO_PERIOD);
            if let Some(camera) = &mut self.camera
                && let Some(sample) = camera.next_sample(capture_id)?
            {
                self.next_audio = true;
                return Ok(Some(sample));
            }
        }
        self.next_audio = self.microphone.is_some();
        Ok(None)
    }

    fn stop(&mut self) {
        self.camera = None;
        self.microphone = None;
        self.video_due = None;
        self.foundation = None;
        self.apartment = None;
    }
}

impl Drop for NativeCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

struct ComApartment;

impl ComApartment {
    fn enter() -> Result<Self, CaptureFailure> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            Ok(Self)
        } else {
            Err(CaptureFailure::Internal)
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct MediaFoundation;

impl MediaFoundation {
    fn start() -> Result<Self, CaptureFailure> {
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }
            .map(|()| Self)
            .map_err(classify_error)
    }
}

impl Drop for MediaFoundation {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
    }
}

fn classify_error(error: WinError) -> CaptureFailure {
    match error.code().0 as u32 {
        0x8007_0005 => CaptureFailure::AccessDenied,
        0x8007_0020 => CaptureFailure::DeviceBusy,
        0xC00D_36D5 => CaptureFailure::NoDevice, // MF_E_NOT_FOUND.
        _ => CaptureFailure::Internal,
    }
}
