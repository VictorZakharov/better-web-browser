//! Shared complete-file admission for the browser's encoded audio consumers.
//! Decoder implementations stay upstream; this layer rejects incomplete or
//! unsupported container shapes before a tolerant demuxer can accept them.

use std::sync::atomic::{AtomicBool, Ordering};
use symphonia::core::codecs::audio::AudioCodecId;
use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_AAC, CODEC_ID_FLAC, CODEC_ID_MP3, CODEC_ID_VORBIS,
};
use symphonia::core::formats::FormatId;
use symphonia::core::formats::well_known::{
    FORMAT_ID_ADTS, FORMAT_ID_ISOMP4, FORMAT_ID_MKV, FORMAT_ID_MP3, FORMAT_ID_OGG,
};

pub(crate) mod adts;
pub(crate) mod ogg_envelope;
mod ogg_flac;
pub(crate) mod webm;

#[cfg(test)]
pub(crate) fn webm_with_overflowing_second_cluster(bytes: &[u8]) -> Vec<u8> {
    webm::overflowing_second_cluster(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Mp3,
    AacM4a,
    AacAdts,
    VorbisWebm,
    FlacOgg,
}

impl Kind {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::AacM4a => "m4a",
            Self::AacAdts => "aac",
            Self::VorbisWebm => "webm",
            Self::FlacOgg => "oga",
        }
    }

    pub(crate) fn expected_format(self) -> FormatId {
        match self {
            Self::Mp3 => FORMAT_ID_MP3,
            Self::AacM4a => FORMAT_ID_ISOMP4,
            Self::AacAdts => FORMAT_ID_ADTS,
            Self::VorbisWebm => FORMAT_ID_MKV,
            Self::FlacOgg => FORMAT_ID_OGG,
        }
    }

    pub(crate) fn expected_codec(self) -> AudioCodecId {
        match self {
            Self::Mp3 => CODEC_ID_MP3,
            Self::AacM4a | Self::AacAdts => CODEC_ID_AAC,
            Self::VorbisWebm => CODEC_ID_VORBIS,
            Self::FlacOgg => CODEC_ID_FLAC,
        }
    }
}

/// Sniff the family, not decoder success: malformed recognized sources must
/// fail in their contained decoder rather than trying an unrelated fallback.
pub(crate) fn sniff(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        Some(Kind::VorbisWebm)
    } else if bytes.starts_with(b"OggS") && ogg_flac::sniff(bytes) {
        Some(Kind::FlacOgg)
    } else if bytes.get(4..8) == Some(b"ftyp") {
        Some(Kind::AacM4a)
    } else if bytes
        .get(..2)
        .is_some_and(|head| head[0] == 0xff && head[1] & 0xf6 == 0xf0)
    {
        Some(Kind::AacAdts)
    } else if bytes.starts_with(b"ID3")
        || bytes.get(..2).is_some_and(|head| {
            head[0] == 0xff
                && head[1] & 0xe0 == 0xe0
                && head[1] & 0x18 != 0x08
                && head[1] & 0x06 == 0x02
        })
    {
        Some(Kind::Mp3)
    } else {
        None
    }
}

pub(crate) fn validate(bytes: &[u8], kind: Kind) -> Result<(), String> {
    validate_inner(bytes, kind, None)
}

pub(crate) fn validate_with_cancel(
    bytes: &[u8],
    kind: Kind,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    validate_inner(bytes, kind, Some(cancelled))
}

fn validate_inner(bytes: &[u8], kind: Kind, cancelled: Option<&AtomicBool>) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES {
        return Err("encoded audio exceeds the shared complete-file limit".into());
    }
    let mut budget = Budget {
        cancelled,
        units: 0,
    };
    budget.step()?;
    match kind {
        Kind::AacAdts => adts::validate(bytes, &mut budget),
        Kind::VorbisWebm => webm::validate(bytes, &mut budget),
        Kind::FlacOgg => ogg_flac::validate(bytes, &mut budget),
        // Preserve the existing consumers' MP3 and ISO BMFF admission/edit policy.
        Kind::Mp3 | Kind::AacM4a => Ok(()),
    }
}

pub(crate) struct Budget<'a> {
    cancelled: Option<&'a AtomicBool>,
    units: usize,
}

impl Budget<'_> {
    pub(crate) fn new(cancelled: Option<&AtomicBool>) -> Budget<'_> {
        Budget {
            cancelled,
            units: 0,
        }
    }
    pub(crate) fn step(&mut self) -> Result<(), String> {
        self.units += 1;
        if self
            .cancelled
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err("audio admission was cancelled".into());
        }
        if self.units > 65_536 {
            return Err("encoded audio framing exceeds admission complexity limit".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_sniff_keeps_malformed_new_formats_in_their_decoder() {
        assert_eq!(sniff(&[0x1a, 0x45, 0xdf, 0xa3]), Some(Kind::VorbisWebm));
        assert_eq!(sniff(&[0xff, 0xf1]), Some(Kind::AacAdts));
        assert_eq!(sniff(b"ID3"), Some(Kind::Mp3));
        assert_eq!(sniff(b"\0\0\0\x18ftyp"), Some(Kind::AacM4a));
        assert_eq!(sniff(b"OggS"), None);
        assert_eq!(sniff(b"RIFF"), None);
    }

    #[test]
    fn admission_has_input_cancellation_and_complexity_bounds() {
        let cancelled = AtomicBool::new(true);
        assert!(validate_with_cancel(b"ID3", Kind::Mp3, &cancelled).is_err());
        assert!(validate(&[], Kind::Mp3).is_err());
        let oversized = vec![0; crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES + 1];
        assert!(validate(&oversized, Kind::VorbisWebm).is_err());
        let mut budget = Budget {
            cancelled: None,
            units: 65_536,
        };
        assert!(budget.step().is_err());
    }

    #[test]
    fn legacy_admission_is_left_to_the_existing_consumers() {
        assert!(validate(b"ID3", Kind::Mp3).is_ok());
        assert!(validate(b"\0\0\0\x18ftyp", Kind::AacM4a).is_ok());
    }
}
