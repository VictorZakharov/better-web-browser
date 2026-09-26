//! Probe only the codecs admitted by the contained media worker.

use super::{ComApartment, MediaFoundation, elapsed_micros};
use crate::media_protocol::{MediaCapabilityReport, MediaLimits};
use std::ptr::null_mut;
use std::time::Instant;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, MFAudioFormat_AAC, MFMediaType_Audio, MFMediaType_Video,
    MFT_CATEGORY_AUDIO_DECODER, MFT_CATEGORY_VIDEO_DECODER, MFT_ENUM_FLAG_ALL,
    MFT_ENUM_FLAG_SORTANDFILTER_WEB_ONLY, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::GUID;

pub(in crate::media_process) fn probe(limits: MediaLimits) -> MediaCapabilityReport {
    let started = Instant::now();
    let _apartment = match ComApartment::initialize() {
        Ok(apartment) => apartment,
        Err(status) => return failed_report(status, started),
    };
    let _foundation = match MediaFoundation::start() {
        Ok(foundation) => foundation,
        Err(status) => return failed_report(status, started),
    };
    let (h264_hresult, h264_decoders) = enumerate_decoders(
        MFT_CATEGORY_VIDEO_DECODER,
        MFMediaType_Video,
        MFVideoFormat_H264,
        limits.max_decoder_candidates,
    );
    let (aac_hresult, aac_decoders) = enumerate_decoders(
        MFT_CATEGORY_AUDIO_DECODER,
        MFMediaType_Audio,
        MFAudioFormat_AAC,
        limits.max_decoder_candidates,
    );
    MediaCapabilityReport {
        startup_hresult: 0,
        h264_hresult,
        aac_hresult,
        h264_decoders,
        aac_decoders,
        probe_micros: elapsed_micros(started),
    }
}

fn failed_report(status: i32, started: Instant) -> MediaCapabilityReport {
    MediaCapabilityReport {
        startup_hresult: status,
        h264_hresult: status,
        aac_hresult: status,
        h264_decoders: 0,
        aac_decoders: 0,
        probe_micros: elapsed_micros(started),
    }
}

fn enumerate_decoders(category: GUID, major: GUID, subtype: GUID, maximum: u16) -> (i32, u16) {
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: major,
        guidSubtype: subtype,
    };
    let mut pointer: *mut Option<IMFActivate> = null_mut();
    let mut count = 0_u32;
    let flags = MFT_ENUM_FLAG_ALL | MFT_ENUM_FLAG_SORTANDFILTER_WEB_ONLY;
    let result = unsafe {
        MFTEnumEx(
            category,
            flags,
            Some(&raw const input),
            None,
            &mut pointer,
            &mut count,
        )
    };
    let _activations = ActivationList { pointer, count };
    match result {
        Ok(()) => (0, count.min(u32::from(maximum)) as u16),
        Err(error) => (error.code().0, 0),
    }
    // `activations` releases every IMFActivate and the COM-allocated array here.
}

pub(super) struct ActivationList {
    pub(super) pointer: *mut Option<IMFActivate>,
    pub(super) count: u32,
}

impl Drop for ActivationList {
    fn drop(&mut self) {
        if self.pointer.is_null() {
            return;
        }
        let activations =
            unsafe { std::slice::from_raw_parts_mut(self.pointer, self.count as usize) };
        for activation in activations {
            drop(activation.take());
        }
        unsafe { CoTaskMemFree(Some(self.pointer.cast())) };
    }
}
