//! Resolve properties of the primary item, never properties of thumbnails/alpha.
//!
//! avif-parse does not expose transformation or color properties. This small
//! adapter inspects only pitm/iprp; compressed data and item references stay in
//! the upstream demuxer. Unsupported essential properties fail closed.

#[path = "aperture.rs"]
mod aperture;
#[cfg(test)]
mod association_tests;
#[path = "boxes.rs"]
mod boxes;
#[cfg(test)]
mod display_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

use super::super::{DecodeLimits, DecodeResult, Orientation, RasterImage};
use aperture::Aperture;
use boxes::{Reader, boxes, unique};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cicp {
    pub primaries: u16,
    pub transfer: u16,
    pub matrix: u16,
    pub full_range: bool,
}

#[derive(Debug)]
enum Transform {
    Orientation(Orientation),
    Aperture(Aperture),
}

#[derive(Debug, Default)]
pub(super) struct Metadata<'a> {
    pub icc: Option<&'a [u8]>,
    pub cicp: Option<Cicp>,
    spatial: Option<(u32, u32)>,
    transforms: Vec<Transform>,
}

impl Metadata<'_> {
    pub fn transform(
        self,
        mut image: RasterImage,
        limits: DecodeLimits,
        ignore_orientation: bool,
    ) -> DecodeResult<RasterImage> {
        if self
            .spatial
            .is_some_and(|size| size != (image.width, image.height))
        {
            return Err("AVIF ispe dimensions do not match decoded pixels".into());
        }
        for transform in self.transforms {
            image = match transform {
                Transform::Orientation(_) if ignore_orientation => image,
                Transform::Orientation(value) => value.apply(image, limits)?,
                Transform::Aperture(value) => value.apply(image, limits)?,
            };
        }
        Ok(image)
    }
}

pub(super) fn parse(bytes: &[u8]) -> DecodeResult<Metadata<'_>> {
    let root = boxes(bytes)?;
    let meta = unique(&root, b"meta")?.ok_or("AVIF has no item metadata")?;
    let mut reader = Reader(meta.data);
    if reader.full_box()? != (0, 0) {
        return Err("unsupported AVIF meta version or flags".into());
    }
    let children = boxes(reader.0)?;
    let pitm = unique(&children, b"pitm")?.ok_or("AVIF has no primary item")?;
    let mut reader = Reader(pitm.data);
    let (version, flags) = reader.full_box()?;
    if flags != 0 || version > 1 {
        return Err("unsupported AVIF primary item header".into());
    }
    let primary = if version == 0 {
        u32::from(reader.u16()?)
    } else {
        reader.u32()?
    };
    reader.end()?;
    let iprp = unique(&children, b"iprp")?.ok_or("AVIF has no item properties")?;
    let properties = boxes(iprp.data)?;
    let ipco = unique(&properties, b"ipco")?.ok_or("AVIF has no property container")?;
    let definitions = boxes(ipco.data)?;
    let mut output = Metadata::default();
    let mut primary_seen = false;
    for association in properties.iter().filter(|value| &value.kind == b"ipma") {
        let mut reader = Reader(association.data);
        let (version, flags) = reader.full_box()?;
        if version > 1 || flags & !1 != 0 {
            return Err("unsupported AVIF property association header".into());
        }
        let entries = reader.u32()?;
        if entries > 1024 {
            return Err("too many AVIF property association entries".into());
        }
        for _ in 0..entries {
            let item = if version == 0 {
                u32::from(reader.u16()?)
            } else {
                reader.u32()?
            };
            let count = reader.u8()?;
            if item == primary {
                if primary_seen {
                    return Err("duplicate AVIF primary item associations".into());
                }
                primary_seen = true;
            }
            for _ in 0..count {
                let raw = if flags & 1 == 0 {
                    u16::from(reader.u8()?)
                } else {
                    reader.u16()?
                };
                let essential_mask = if flags & 1 == 0 { 0x80 } else { 0x8000 };
                let index = raw & !essential_mask;
                if index == 0 {
                    continue;
                }
                let property = definitions
                    .get(index as usize - 1)
                    .ok_or("AVIF property index is out of range")?;
                if item == primary {
                    apply_property(
                        &mut output,
                        property.kind,
                        property.data,
                        raw & essential_mask != 0,
                    )?;
                }
            }
        }
        reader.end()?;
    }
    if !primary_seen {
        return Err("AVIF primary item has no property associations".into());
    }
    Ok(output)
}

fn apply_property<'a>(
    output: &mut Metadata<'a>,
    kind: [u8; 4],
    bytes: &'a [u8],
    essential: bool,
) -> DecodeResult<()> {
    match &kind {
        b"ispe" => {
            let mut reader = Reader(bytes);
            if reader.full_box()? != (0, 0) {
                return Err("invalid AVIF spatial property".into());
            }
            let size = (reader.u32()?, reader.u32()?);
            reader.end()?;
            if output.spatial.replace(size).is_some() {
                return Err("duplicate AVIF spatial property".into());
            }
        }
        b"irot" => {
            if bytes.len() != 1 || bytes[0] > 3 {
                return Err("invalid AVIF rotation property".into());
            }
            // HEIF irot is counter-clockwise in units of 90 degrees.
            output.transforms.push(Transform::Orientation(Orientation(
                [1, 8, 3, 6][bytes[0] as usize],
            )));
        }
        b"imir" => {
            if bytes.len() != 1 || bytes[0] > 1 {
                return Err("invalid AVIF mirror property".into());
            }
            output
                .transforms
                .push(Transform::Orientation(Orientation(if bytes[0] == 0 {
                    2
                } else {
                    4
                })));
        }
        b"clap" => output
            .transforms
            .push(Transform::Aperture(Aperture::parse(bytes)?)),
        b"colr" => {
            let mut reader = Reader(bytes);
            match reader.take(4)? {
                b"nclx" => {
                    let value = Cicp {
                        primaries: reader.u16()?,
                        transfer: reader.u16()?,
                        matrix: reader.u16()?,
                        full_range: reader.u8()? & 0x80 != 0,
                    };
                    reader.end()?;
                    if output.cicp.replace(value).is_some() {
                        return Err("duplicate AVIF CICP property".into());
                    }
                }
                b"prof" | b"rICC" => {
                    if reader.0.len() > 4 * 1024 * 1024 {
                        return Err("AVIF ICC profile exceeds the metadata budget".into());
                    }
                    if output.icc.replace(reader.0).is_some() {
                        return Err("duplicate AVIF ICC property".into());
                    }
                }
                _ if essential => return Err("unsupported essential AVIF color property".into()),
                _ => (),
            }
        }
        b"pasp" => {
            let mut reader = Reader(bytes);
            let horizontal = reader.u32()?;
            let vertical = reader.u32()?;
            reader.end()?;
            if horizontal == 0 || horizontal != vertical {
                return Err("non-square AVIF pixel aspect ratio is unsupported".into());
            }
        }
        // Configuration, channel depths and content-light metadata are handled
        // by avif-parse/rav1d. Do not mistake them for display transforms.
        b"av1C" | b"pixi" | b"clli" | b"mdcv" | b"auxC" => (),
        _ if essential => {
            return Err(format!(
                "unsupported essential AVIF property {}",
                String::from_utf8_lossy(&kind)
            ));
        }
        _ => (),
    }
    Ok(())
}
