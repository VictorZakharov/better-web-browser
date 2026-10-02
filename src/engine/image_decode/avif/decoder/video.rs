//! Stateful AV1 reuse of the still-image ownership and color conversion boundary.
use super::*;
use crate::engine::image_decode::RasterImage;
use std::sync::atomic::{AtomicBool, Ordering};
mod framing;

pub(crate) struct VideoPicture {
    pub image: RasterImage,
    pub timestamp: i64,
    pub duration: Option<u64>,
}

pub(crate) struct VideoAv1Decoder {
    context: Context,
    limits: DecodeLimits,
}

impl VideoAv1Decoder {
    pub(crate) fn new() -> DecodeResult<Self> {
        let limits = DecodeLimits {
            source_bytes: 4 * 1024 * 1024,
            dimension: 1920,
            pixels: 1920 * 1080,
            working_bytes: 8 * 1024 * 1024,
        };
        let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
        // SAFETY: rav1d initializes the whole settings object.
        let mut settings = unsafe {
            dav1d_default_settings(NonNull::new(settings.as_mut_ptr()).unwrap());
            settings.assume_init()
        };
        // Decode remains in one bounded renderer-owned thread. No auxiliary
        // thread pool or hardware/platform escape is opened by this path.
        settings.n_threads = 1;
        settings.max_frame_delay = 1;
        settings.frame_size_limit = limits.pixels as u32;
        settings.strict_std_compliance = 1;
        settings.all_layers = 0;
        // SAFETY: a null callback and cookie cannot invoke author code.
        settings.logger = unsafe { Dav1dLogger::new(None, None) };
        let mut context = Context(None);
        // SAFETY: valid, uniquely owned out-param and settings for this call.
        let status = unsafe {
            dav1d_open(
                Some(NonNull::from(&mut context.0)),
                Some(NonNull::from(&mut settings)),
            )
        };
        if status.0 != 0 {
            return Err(format!("open AV1 video decoder: {}", status.0));
        }
        Ok(Self { context, limits })
    }

    pub(crate) fn decode(
        &mut self,
        bytes: &[u8],
        timestamp: i64,
        duration: Option<u64>,
        key: bool,
        cancelled: &AtomicBool,
    ) -> DecodeResult<Vec<VideoPicture>> {
        self.limits.check_source(bytes)?;
        framing::validate(bytes, key)?;
        if cancelled.load(Ordering::Acquire) {
            return Err("AV1 decode was cancelled".into());
        }
        let mut data = Data(Dav1dData::default());
        // SAFETY: initialized owned Data receives an allocation of this exact length.
        let destination =
            unsafe { dav1d_data_create(Some(NonNull::from(&mut data.0)), bytes.len()) };
        if destination.is_null() {
            return Err("allocate AV1 video packet".into());
        }
        // SAFETY: the fresh allocation and source are disjoint and contain bytes.len().
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
        }
        data.0.m.timestamp = timestamp;
        data.0.m.duration = duration.map_or(-1, |value| value as i64);
        let mut output = Vec::new();
        let mut output_bytes = 0;
        for _ in 0..64 {
            if cancelled.load(Ordering::Acquire) {
                return Err("AV1 decode was cancelled".into());
            }
            let remaining = data.0.sz;
            // SAFETY: live context and owned Data; rav1d clears consumed storage.
            let status =
                unsafe { dav1d_send_data(self.context.0, Some(NonNull::from(&mut data.0))) };
            if status.0 < 0 && status.0 != -11 {
                return Err(format!("decode AV1 packet: {}", status.0));
            }
            let before = output.len();
            self.receive(&mut output, &mut output_bytes, key, cancelled)?;
            if data.0.sz == 0 {
                return Ok(output);
            }
            if data.0.sz >= remaining && output.len() == before {
                return Err("AV1 packet made no decoder progress".into());
            }
        }
        Err("AV1 packet exceeds its decode iteration budget".into())
    }

    pub(crate) fn flush(&mut self, cancelled: &AtomicBool) -> DecodeResult<Vec<VideoPicture>> {
        let mut output = Vec::new();
        self.receive(&mut output, &mut 0, false, cancelled)?;
        Ok(output)
    }

    fn receive(
        &mut self,
        output: &mut Vec<VideoPicture>,
        output_bytes: &mut usize,
        key: bool,
        cancelled: &AtomicBool,
    ) -> DecodeResult<()> {
        for _ in 0..64 {
            if cancelled.load(Ordering::Acquire) {
                return Err("AV1 decode was cancelled".into());
            }
            let mut picture = Picture(Dav1dPicture::default());
            // SAFETY: the empty output picture becomes uniquely owned on success.
            let status =
                unsafe { dav1d_get_picture(self.context.0, Some(NonNull::from(&mut picture.0))) };
            if status.0 == -11 {
                return Ok(());
            }
            if status.0 < 0 {
                return Err(format!("receive AV1 picture: {}", status.0));
            }
            // SAFETY: Picture retains both referenced headers through conversion.
            let (sequence, frame) = unsafe {
                (
                    picture
                        .0
                        .seq_hdr
                        .ok_or("AV1 picture lacks sequence metadata")?
                        .as_ref(),
                    picture
                        .0
                        .frame_hdr
                        .ok_or("AV1 picture lacks frame metadata")?
                        .as_ref(),
                )
            };
            if sequence.profile != 0 || picture.0.p.bpc != 8 {
                return Err("AV1 video baseline requires profile 0 and 8-bit pictures".into());
            }
            if matches!(sequence.trc, 16 | 18) {
                return Err("AV1 video baseline does not implement HDR presentation".into());
            }
            if key && frame.frame_type != rav1d::include::dav1d::headers::DAV1D_FRAME_TYPE_KEY {
                return Err("AV1 key chunk does not contain a KEY_FRAME".into());
            }
            let mut image = super::super::pixels::rgba(&picture, self.limits, None)?;
            super::super::pixels::apply_color_encoding(&mut image, &picture, None)?;
            *output_bytes = output_bytes
                .checked_add(image.rgba.len())
                .filter(|size| *size <= 8 * 1024 * 1024)
                .ok_or("AV1 output batch exceeds 8 MiB")?;
            output.push(VideoPicture {
                image,
                timestamp: picture.0.m.timestamp,
                duration: u64::try_from(picture.0.m.duration).ok(),
            });
        }
        Err("AV1 packet exceeds 64 output pictures".into())
    }
}
