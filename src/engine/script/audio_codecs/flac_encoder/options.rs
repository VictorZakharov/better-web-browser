//! Real compression-effort presets over flacenc's supported coding controls.
use super::super::config::Config;
use flacenc::error::{Verified, Verify};

pub(in crate::engine::script::audio_codecs) fn validate(config: &Config) -> Result<(), String> {
    let options = config.flac.clone().unwrap_or_default();
    if !(8_000..=96_000).contains(&config.sample_rate)
        || !(1..=2).contains(&config.number_of_channels)
        || config.opus.is_some()
        || config.description.is_some()
        || config.bitrate.is_some()
        || config
            .bitrate_mode
            .as_deref()
            .is_some_and(|value| value != "variable")
        || (options.block_size != 0 && !(32..=32767).contains(&options.block_size))
        || options.compress_level > 8
    {
        return Err("FLAC encoder requires 8–96 kHz mono/stereo, variable bitrate, blocks 32–32767 or auto, and compression 0–8".into());
    }
    Ok(())
}

pub(super) fn backend(
    block_size: usize,
    level: u32,
) -> Result<Verified<flacenc::config::Encoder>, String> {
    let mut config = flacenc::config::Encoder::default();
    config.block_size = block_size;
    config.multithread = false; // The realm-owned worker already bounds concurrency.
    let stereo = level > 0;
    config.stereo_coding.use_leftside = stereo;
    config.stereo_coding.use_rightside = stereo;
    config.stereo_coding.use_midside = stereo;
    config.subframe_coding.use_fixed = level > 0;
    config.subframe_coding.fixed.max_order = if level < 2 { 2 } else { 4 };
    config.subframe_coding.use_lpc = level >= 3;
    // These are Breeze effort levels, not a claim to reproduce libFLAC's
    // implementation-specific presets. Every accepted level changes actual
    // predictor search or Rice parameter work; none is a ignored support flag.
    config.subframe_coding.qlpc.lpc_order = [6, 6, 6, 6, 8, 12, 16, 20, 24][level as usize];
    config.subframe_coding.prc.max_parameter = [8, 10, 12, 12, 13, 14, 14, 14, 14][level as usize];
    config
        .into_verified()
        .map_err(|error| format!("invalid FLAC backend options: {error:?}"))
}
