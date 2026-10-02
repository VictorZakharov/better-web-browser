//! RFC 6716 packet durations, assembled by libopus rather than a local bitstream writer.
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn encode(
    codec: &mut opus::Encoder,
    samples: &[f32],
    rate: u32,
    channels: usize,
    duration: u32,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, String> {
    if matches!(duration, 2_500 | 5_000 | 10_000 | 20_000 | 40_000 | 60_000) {
        return codec.encode_vec_float(samples, 4_000).map_err(codec_error);
    }
    // The encoder accepts six native call durations. Other valid packet lengths
    // use homogeneous 2.5 ms CELT frames so adaptive SILK/CELT mode changes cannot
    // make consecutive TOCs incompatible with the upstream repacketizer.
    let frame_samples = rate as usize / 400 * channels;
    if !(2_500..=120_000).contains(&duration)
        || !duration.is_multiple_of(2_500)
        || frame_samples == 0
        || samples.len() != frame_samples * (duration / 2_500) as usize
    {
        return Err("invalid Opus packet sample layout".into());
    }
    let mut pieces = Vec::with_capacity((duration / 2_500) as usize);
    for frame in samples.chunks_exact(frame_samples) {
        if cancelled.load(Ordering::Acquire) {
            return Err("Opus repacketization cancelled".into());
        }
        pieces.push(codec.encode_vec_float(frame, 4_000).map_err(codec_error)?);
    }
    let packets = pieces.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let mut repacketizer = opus::Repacketizer::new().map_err(codec_error)?;
    let mut bytes = vec![0; super::MAX_PACKET_BYTES];
    let size = repacketizer
        .combine(&packets, &mut bytes)
        .map_err(codec_error)?;
    bytes.truncate(size);
    Ok(bytes)
}

fn codec_error(error: opus::Error) -> String {
    format!("Opus packet assembly: {error}")
}

#[cfg(test)]
mod tests;
