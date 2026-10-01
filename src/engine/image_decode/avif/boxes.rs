//! Read-only, bounded ISO BMFF metadata traversal. avif-parse owns item extraction.

use super::super::super::DecodeResult;

#[derive(Clone, Copy, Debug)]
pub(super) struct BoxView<'a> {
    pub kind: [u8; 4],
    pub data: &'a [u8],
}

pub(super) fn boxes(mut bytes: &[u8]) -> DecodeResult<Vec<BoxView<'_>>> {
    let mut output = Vec::new();
    while !bytes.is_empty() {
        if output.len() >= 1024 {
            return Err("AVIF metadata has too many boxes".into());
        }
        let header = bytes.get(..8).ok_or("truncated AVIF box header")?;
        let small = u32::from_be_bytes(header[..4].try_into().unwrap());
        let (size, offset) = match small {
            0 => (bytes.len(), 8),
            1 => {
                let large = bytes.get(8..16).ok_or("truncated AVIF extended box size")?;
                (
                    usize::try_from(u64::from_be_bytes(large.try_into().unwrap()))
                        .map_err(|_| "AVIF box size overflow")?,
                    16,
                )
            }
            size => (size as usize, 8),
        };
        if size < offset || size > bytes.len() {
            return Err("AVIF box extends outside its parent".into());
        }
        output.push(BoxView {
            kind: header[4..8].try_into().unwrap(),
            data: &bytes[offset..size],
        });
        bytes = &bytes[size..];
    }
    Ok(output)
}

pub(super) fn unique<'a>(
    boxes: &[BoxView<'a>],
    kind: &[u8; 4],
) -> DecodeResult<Option<BoxView<'a>>> {
    let mut matching = boxes.iter().filter(|value| &value.kind == kind);
    let found = matching.next().copied();
    if matching.next().is_some() {
        return Err(format!(
            "duplicate AVIF {} box",
            String::from_utf8_lossy(kind)
        ));
    }
    Ok(found)
}

pub(super) struct Reader<'a>(pub &'a [u8]);

impl<'a> Reader<'a> {
    pub fn take(&mut self, size: usize) -> DecodeResult<&'a [u8]> {
        let bytes = self.0.get(..size).ok_or("truncated AVIF metadata")?;
        self.0 = &self.0[size..];
        Ok(bytes)
    }
    pub fn u8(&mut self) -> DecodeResult<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> DecodeResult<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> DecodeResult<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn full_box(&mut self) -> DecodeResult<(u8, u32)> {
        let bytes = self.take(4)?;
        Ok((
            bytes[0],
            u32::from_be_bytes([0, bytes[1], bytes[2], bytes[3]]),
        ))
    }
    pub fn end(self) -> DecodeResult<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err("unexpected trailing AVIF metadata".into())
        }
    }
}
