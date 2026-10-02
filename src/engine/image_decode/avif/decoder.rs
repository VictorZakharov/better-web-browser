//! Ownership adapter for rav1d's C-shaped Rust API. No raw plane outlives Picture.

use super::super::{DecodeLimits, DecodeResult};
use rav1d::include::dav1d::{
    data::Dav1dData,
    dav1d::{Dav1dContext, Dav1dLogger, Dav1dSettings},
    picture::Dav1dPicture,
};
use rav1d::src::lib::*;
use std::{mem::MaybeUninit, ptr::NonNull};

mod video;
pub(crate) use video::VideoAv1Decoder;

struct Context(Option<Dav1dContext>);
struct Data(Dav1dData);
pub(super) struct Picture(pub Dav1dPicture);

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: this owns the context opened below, and closes it exactly once.
        unsafe {
            dav1d_close(Some(NonNull::from(&mut self.0)));
        }
    }
}
impl Drop for Data {
    fn drop(&mut self) {
        // SAFETY: this owns initialized data; send_data clears consumed storage.
        unsafe {
            dav1d_data_unref(Some(NonNull::from(&mut self.0)));
        }
    }
}
impl Drop for Picture {
    fn drop(&mut self) {
        // SAFETY: this owns the returned picture, including all referenced planes.
        unsafe {
            dav1d_picture_unref(Some(NonNull::from(&mut self.0)));
        }
    }
}

pub(super) fn decode(bytes: &[u8], limits: DecodeLimits) -> DecodeResult<Picture> {
    if bytes.is_empty() || bytes.len() > limits.source_bytes {
        return Err("AV1 item exceeds the encoded image budget".into());
    }
    let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
    // SAFETY: the library initializes every field of this correctly aligned object.
    let mut settings = unsafe {
        dav1d_default_settings(NonNull::new(settings.as_mut_ptr()).unwrap());
        settings.assume_init()
    };
    settings.n_threads = 1;
    settings.max_frame_delay = 1;
    settings.frame_size_limit =
        u32::try_from(limits.pixels).map_err(|_| "AV1 frame limit overflow")?;
    settings.strict_std_compliance = 1;
    settings.all_layers = 0;
    // SAFETY: no callback or cookie means no external callback can be invoked.
    settings.logger = unsafe { Dav1dLogger::new(None, None) };
    let mut context = Context(None);
    // SAFETY: both out-param and settings are valid for the duration of this call.
    let status = unsafe {
        dav1d_open(
            Some(NonNull::from(&mut context.0)),
            Some(NonNull::from(&mut settings)),
        )
    };
    if status.0 != 0 {
        return Err(format!("open AV1 decoder: {}", status.0));
    }
    let mut data = Data(Dav1dData::default());
    // SAFETY: data_create owns a buffer of the requested size in the initialized Data.
    let destination = unsafe { dav1d_data_create(Some(NonNull::from(&mut data.0)), bytes.len()) };
    if destination.is_null() {
        return Err("allocate AV1 item buffer".into());
    }
    // SAFETY: source and fresh codec allocation are disjoint and both contain bytes.len().
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
    }
    let mut picture = Picture(Dav1dPicture::default());
    // A still item may contain several OBUs. Bound the send/get loop by consumed
    // bytes and refuse no-progress retries instead of spinning on EAGAIN.
    for _ in 0..64 {
        let remaining = data.0.sz;
        // SAFETY: context is live, Data is owned and writable; rav1d may consume it.
        let sent = unsafe { dav1d_send_data(context.0, Some(NonNull::from(&mut data.0))) };
        // SAFETY: picture is an empty owned writable output object.
        let got = unsafe { dav1d_get_picture(context.0, Some(NonNull::from(&mut picture.0))) };
        if got.0 == 0 {
            limits.rgba_len(
                u32::try_from(picture.0.p.w).map_err(|_| "invalid AV1 width")?,
                u32::try_from(picture.0.p.h).map_err(|_| "invalid AV1 height")?,
            )?;
            return Ok(picture);
        }
        if sent.0 < 0 || data.0.sz == 0 || data.0.sz >= remaining {
            return Err(format!(
                "decode AV1 image: send {}, receive {}",
                sent.0, got.0
            ));
        }
    }
    Err("AV1 still image exceeds the bounded decode iteration budget".into())
}
