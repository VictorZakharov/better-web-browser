//! Vorbis registration uses Xiph-laced identification/comments/setup metadata.
//! Reuse the preflight already protecting Web Audio and media decode codebooks.
use symphonia::core::codecs::audio::{AudioCodecParameters, well_known::CODEC_ID_VORBIS};

pub(super) fn parameters(description: Option<&[u8]>) -> Result<AudioCodecParameters, String> {
    let bytes =
        description.ok_or("Vorbis description with three Xiph-laced headers is required")?;
    if bytes.len() > 65_536 {
        return Err("Vorbis description exceeds 64 KiB".into());
    }
    crate::ogg_vorbis_headers::preflight_xiph_laced(bytes, 8)?;
    let mut parameters = AudioCodecParameters::new();
    parameters
        .for_codec(CODEC_ID_VORBIS)
        .with_extra_data(bytes.to_vec().into_boxed_slice());
    Ok(parameters)
}
