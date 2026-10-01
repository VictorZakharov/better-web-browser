//! Inspect allocation-driving metadata through upstream parsers before decoding.
//!
//! JPEG reconstruction, XML, Exif and compressed auxiliary boxes are not needed
//! to paint pixels. Do not pass them to oxide's unbounded Brotli metadata writer.
//! The codestream itself contains the normative orientation and color encoding.

use super::super::{DecodeLimits, DecodeResult};
use jxl_bitstream::{Bitstream, ContainerParser, ParseEvent};
use jxl_oxide::ImageHeader;
use jxl_oxide_common::Bundle;
use std::borrow::Cow;

const METADATA_BYTES: usize = 4 * 1024 * 1024;

pub(super) fn codestream(bytes: &[u8], limits: DecodeLimits) -> DecodeResult<Cow<'_, [u8]>> {
    if bytes.starts_with(&[0xff, 0x0a]) {
        return Ok(Cow::Borrowed(bytes));
    }
    check_container_extents(bytes)?;
    let mut parser = ContainerParser::new();
    let mut output = Vec::new();
    for event in parser.feed_bytes(bytes) {
        if let ParseEvent::Codestream(part) =
            event.map_err(|error| format!("JPEG XL container: {error}"))?
        {
            let length = output
                .len()
                .checked_add(part.len())
                .ok_or("JPEG XL codestream size overflow")?;
            if length > limits.source_bytes {
                return Err("JPEG XL codestream exceeds source budget".into());
            }
            output.extend_from_slice(part);
        }
    }
    if parser.previous_consumed_bytes() != bytes.len() || !output.starts_with(&[0xff, 0x0a]) {
        return Err("JPEG XL container has no complete codestream".into());
    }
    Ok(Cow::Owned(output))
}

fn check_container_extents(mut bytes: &[u8]) -> DecodeResult<()> {
    let mut count = 0;
    let mut partial_last = false;
    let mut partial_seen = false;
    while !bytes.is_empty() {
        count += 1;
        if count > 1024 {
            return Err("JPEG XL has too many container boxes".into());
        }
        let header = bytes.get(..8).ok_or("truncated JPEG XL box header")?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap());
        let (length, offset) = match size {
            0 => (bytes.len(), 8),
            1 => {
                let value = bytes
                    .get(8..16)
                    .ok_or("truncated JPEG XL extended box header")?;
                (
                    usize::try_from(u64::from_be_bytes(value.try_into().unwrap()))
                        .map_err(|_| "JPEG XL box size overflow")?,
                    16,
                )
            }
            value => (value as usize, 8),
        };
        if length < offset || length > bytes.len() {
            return Err("JPEG XL box exceeds source extent".into());
        }
        if &header[4..8] == b"jxlp" {
            if partial_last {
                return Err("JPEG XL partial codestream follows its final part".into());
            }
            let index = bytes
                .get(offset..offset + 4)
                .filter(|_| length >= offset + 4)
                .ok_or("truncated JPEG XL partial index")?;
            partial_seen = true;
            partial_last = u32::from_be_bytes(index.try_into().unwrap()) & 0x80000000 != 0;
        }
        bytes = &bytes[length..];
    }
    if partial_seen && !partial_last {
        return Err("JPEG XL partial codestream lacks a final part".into());
    }
    Ok(())
}

pub(super) fn header(bytes: &[u8], limits: DecodeLimits) -> DecodeResult<()> {
    let mut bits = Bitstream::new(bytes);
    let header = ImageHeader::parse(&mut bits, ())
        .map_err(|error| format!("JPEG XL image header: {error}"))?;
    limits.rgba_len(
        header.width_with_orientation(),
        header.height_with_orientation(),
    )?;
    // A preview is another allocation-driving frame, independent of the main
    // image dimensions. Baseline decoding deliberately admits only no-preview
    // images until the upstream API exposes a bounded preview-header interface.
    if header.metadata.preview.is_some() {
        return Err("JPEG XL preview decoding is not yet supported".into());
    }
    if header.metadata.colour_encoding.want_icc() {
        let mut size_bits = bits.clone();
        let encoded_size = size_bits
            .read_u64()
            .map_err(|error| format!("JPEG XL ICC size: {error}"))?;
        if encoded_size > METADATA_BYTES as u64 {
            return Err("JPEG XL encoded ICC exceeds metadata budget".into());
        }
        let encoded = jxl_color::icc::read_icc(&mut bits)
            .map_err(|error| format!("JPEG XL ICC stream: {error}"))?;
        // The entropy-decoded ICC stream begins with the reconstructed profile
        // length, before its commands. Check it before oxide expands the profile.
        if profile_length(&encoded)? > METADATA_BYTES as u64 {
            return Err("JPEG XL reconstructed ICC exceeds metadata budget".into());
        }
    }
    Ok(())
}

fn profile_length(bytes: &[u8]) -> DecodeResult<u64> {
    let mut output = 0u64;
    for (index, &value) in bytes.iter().take(10).enumerate() {
        if index == 9 && value > 1 {
            return Err("JPEG XL ICC length overflow".into());
        }
        output |= u64::from(value & 0x7f) << (index * 7);
        if value & 0x80 == 0 {
            return Ok(output);
        }
    }
    Err("truncated JPEG XL ICC length".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconstructed_icc_size_cannot_wrap_or_accept_truncation() {
        assert_eq!(profile_length(&[0]).unwrap(), 0);
        assert_eq!(profile_length(&[0x80, 0x20]).unwrap(), 4096);
        assert_eq!(
            profile_length(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1]).unwrap(),
            u64::MAX
        );
        for bytes in [&[][..], &[0x80], &[0x80; 10], &[0xff; 10]] {
            assert!(profile_length(bytes).is_err());
        }
    }

    #[test]
    fn declared_container_boxes_must_fit_the_encoded_source() {
        for bytes in [
            &b"\0\0\0\x07junk"[..],
            b"\0\0\0\x09junk",
            b"\0\0\0\x01junk",
            b"\0\0\0\x08junk\0",
        ] {
            assert!(check_container_extents(bytes).is_err());
        }
        assert!(check_container_extents(b"\0\0\0\x08junk").is_ok());
        assert!(check_container_extents(b"\0\0\0\0junkrest").is_ok());
    }
}
