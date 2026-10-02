//! AAC-LC registration: ASC selects raw_data_block; no ASC selects ADTS.
use symphonia::core::codecs::audio::{AudioCodecParameters, well_known::CODEC_ID_AAC};
use symphonia_common::mpeg::audio::{AudioObjectType, AudioSpecificConfig};

pub(super) fn parameters(bytes: &[u8]) -> Result<AudioCodecParameters, String> {
    if !(2..=64).contains(&bytes.len()) {
        return Err("AAC AudioSpecificConfig must occupy 2–64 bytes".into());
    }
    let asc = AudioSpecificConfig::read(bytes)
        .map_err(|error| format!("invalid AAC AudioSpecificConfig: {error}"))?;
    if asc.object_type != AudioObjectType::Lc
        || asc.sbr_present
        || asc.ps_present
        || asc.samples != 1024
        || !asc
            .channels
            .as_ref()
            .is_some_and(|c| (1..=2).contains(&c.count()))
        || !(8_000..=96_000).contains(&asc.sample_rate)
    {
        return Err("AAC decoder supports 1024-sample AAC-LC mono/stereo without SBR/PS".into());
    }
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_AAC)
        .with_extra_data(bytes.to_vec().into_boxed_slice());
    Ok(params)
}

pub(super) fn adts(bytes: &[u8]) -> Result<crate::encoded_audio::adts::Frame<'_>, String> {
    let frame = crate::encoded_audio::adts::frame(bytes)?;
    if frame.bytes != bytes.len() || frame.channels > 2 {
        return Err("AAC chunk requires exactly one mono/stereo ADTS frame".into());
    }
    // Do not silently discard an integrity checksum the decoder cannot verify.
    // CRC-protected ADTS remains explicitly unsupported in this packet adapter.
    if frame.protected {
        return Err("CRC-protected ADTS packets are not supported".into());
    }
    Ok(frame)
}
