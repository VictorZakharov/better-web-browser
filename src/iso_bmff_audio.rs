//! Bounded ISO BMFF audio edit-list inspection shared by Web Audio and media playback.
//!
//! Symphonia's ISO BMFF demuxer currently decodes the media timeline without
//! applying `edts/elst`. Only a single, nonempty, unity-rate edit with exact
//! sample-frame boundaries is accepted here; other timelines must use a
//! different decoder rather than reporting the untrimmed AAC priming as audio.

const MAX_BOXES_PER_LEVEL: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    movie_timescale: u32,
    media_timescale: u32,
    media_duration: u64,
    media_time: u64,
    segment_duration: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameWindow {
    pub start: u64,
    pub end: u64,
}

impl Edit {
    /// Return a half-open window in the decoded track's source sample frames.
    /// Per the QuickTime encoder-delay convention, `media_time` skips priming
    /// in media time while `segment_duration` gives playable movie time.
    /// Fractional boundaries cannot be represented sample-accurately and are
    /// rejected instead of silently rounding playback duration or seek points.
    pub(crate) fn frame_window(self, sample_rate: u32) -> Result<FrameWindow, String> {
        let start = exact_frames(self.media_time, sample_rate, self.media_timescale)?;
        let duration = exact_frames(self.segment_duration, sample_rate, self.movie_timescale)?;
        let end = start
            .checked_add(duration)
            .ok_or("ISO BMFF edit frame window overflow")?;
        if sample_rate == 0 || duration == 0 {
            return Err("ISO BMFF edit has no playable sample frames".into());
        }
        // The edit must fit the declared media timeline, even if AAC decoding
        // produces extra priming/padding samples around that timeline.
        let end_media = u128::from(self.media_time)
            .checked_mul(u128::from(self.movie_timescale))
            .and_then(|start| {
                u128::from(self.segment_duration)
                    .checked_mul(u128::from(self.media_timescale))
                    .and_then(|duration| start.checked_add(duration))
            })
            .ok_or("ISO BMFF edit media duration overflow")?;
        let available = u128::from(self.media_duration) * u128::from(self.movie_timescale);
        if end_media > available {
            return Err("ISO BMFF edit extends beyond the media timeline".into());
        }
        Ok(FrameWindow { start, end })
    }
}

fn exact_frames(ticks: u64, rate: u32, timescale: u32) -> Result<u64, String> {
    if rate == 0 || timescale == 0 {
        return Err("ISO BMFF edit has an invalid timescale".into());
    }
    let scaled = u128::from(ticks) * u128::from(rate);
    if scaled % u128::from(timescale) != 0 {
        return Err("ISO BMFF edit boundary falls between sample frames".into());
    }
    u64::try_from(scaled / u128::from(timescale))
        .map_err(|_| "ISO BMFF edit frame offset overflow".into())
}

#[derive(Clone, Copy)]
struct Atom<'a> {
    kind: [u8; 4],
    payload: &'a [u8],
}

struct Atoms<'a> {
    bytes: &'a [u8],
    offset: usize,
    count: usize,
}

fn atoms(bytes: &[u8]) -> Atoms<'_> {
    Atoms {
        bytes,
        offset: 0,
        count: 0,
    }
}

impl<'a> Iterator for Atoms<'a> {
    type Item = Result<Atom<'a>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.bytes.len() {
            return None;
        }
        self.count += 1;
        if self.count > MAX_BOXES_PER_LEVEL {
            self.offset = self.bytes.len();
            return Some(Err("ISO BMFF box count exceeds inspection limit".into()));
        }
        let result = self.read_next();
        if result.is_err() {
            self.offset = self.bytes.len();
        }
        Some(result)
    }
}

impl<'a> Atoms<'a> {
    fn read_next(&mut self) -> Result<Atom<'a>, String> {
        let header = self
            .bytes
            .get(self.offset..self.offset.saturating_add(8))
            .ok_or("truncated ISO BMFF box header")?;
        let kind: [u8; 4] = header[4..8].try_into().unwrap();
        let short_size = u32::from_be_bytes(header[..4].try_into().unwrap());
        let header_size = if short_size == 1 { 16 } else { 8 };
        let size = match short_size {
            0 => self.bytes.len() - self.offset,
            1 => {
                let extended = self
                    .bytes
                    .get(self.offset + 8..self.offset + 16)
                    .ok_or("truncated extended ISO BMFF box header")?;
                usize::try_from(u64::from_be_bytes(extended.try_into().unwrap()))
                    .map_err(|_| "ISO BMFF box size overflows address space")?
            }
            value => value as usize,
        };
        let end = self
            .offset
            .checked_add(size)
            .ok_or("ISO BMFF box end overflows address space")?;
        if size < header_size || end > self.bytes.len() {
            return Err("ISO BMFF box exceeds its parent".into());
        }
        let payload = &self.bytes[self.offset + header_size..end];
        self.offset = end;
        Ok(Atom { kind, payload })
    }
}

/// Inspect a complete ordinary MP4, rejecting fragments and ambiguous track
/// mappings. `Ok(None)` means no edit list is present, not an unsupported edit.
pub(crate) fn ordinary_audio_edit(bytes: &[u8]) -> Result<Option<Edit>, String> {
    let mut top = atoms(bytes);
    if top
        .next()
        .transpose()?
        .is_none_or(|atom| atom.kind != *b"ftyp")
    {
        return Err("ISO BMFF source does not start with ftyp".into());
    }
    let mut moov = None;
    for atom in top {
        let atom = atom?;
        match &atom.kind {
            b"moof" => return Err("fragmented ISO BMFF is unsupported".into()),
            b"moov" if moov.replace(atom.payload).is_some() => {
                return Err("ISO BMFF source has multiple movie boxes".into());
            }
            _ => {}
        }
    }
    let moov = moov.ok_or("ISO BMFF source has no movie box")?;
    let mut mvhd = None;
    let mut trak = None;
    for atom in atoms(moov) {
        let atom = atom?;
        match &atom.kind {
            b"mvex" => return Err("fragmented ISO BMFF is unsupported".into()),
            b"mvhd" if mvhd.replace(atom.payload).is_some() => {
                return Err("ISO BMFF source has multiple movie headers".into());
            }
            b"trak" if trak.replace(atom.payload).is_some() => {
                return Err("ISO BMFF source has multiple tracks".into());
            }
            _ => {}
        }
    }
    let trak = trak.ok_or("ISO BMFF source has no track")?;
    let mut edts = None;
    let mut mdia = None;
    for atom in atoms(trak) {
        let atom = atom?;
        match &atom.kind {
            b"edts" if edts.replace(atom.payload).is_some() => {
                return Err("ISO BMFF track has multiple edit boxes".into());
            }
            b"mdia" if mdia.replace(atom.payload).is_some() => {
                return Err("ISO BMFF track has multiple media boxes".into());
            }
            _ => {}
        }
    }
    let Some(edts) = edts else {
        return Ok(None);
    };
    let movie_timescale = timescale(mvhd.ok_or("ISO BMFF edit has no movie header")?)?.0;
    let mdia = mdia.ok_or("ISO BMFF edit has no media box")?;
    let mut mdhd = None;
    for atom in atoms(mdia) {
        let atom = atom?;
        if atom.kind == *b"mdhd" && mdhd.replace(atom.payload).is_some() {
            return Err("ISO BMFF edit has multiple media headers".into());
        }
    }
    let (media_timescale, media_duration) =
        timescale(mdhd.ok_or("ISO BMFF edit has no media header")?)?;
    let mut elst = None;
    for atom in atoms(edts) {
        let atom = atom?;
        if atom.kind == *b"elst" && elst.replace(atom.payload).is_some() {
            return Err("ISO BMFF edit has multiple edit lists".into());
        }
    }
    let (segment_duration, media_time) =
        edit_entry(elst.ok_or("ISO BMFF edit box has no edit list")?)?;
    Ok(Some(Edit {
        movie_timescale,
        media_timescale,
        media_duration,
        media_time,
        segment_duration,
    }))
}

fn timescale(payload: &[u8]) -> Result<(u32, u64), String> {
    let (time_offset, duration_offset, duration_size) = match payload.first() {
        Some(0) => (12, 16, 4),
        Some(1) => (20, 24, 8),
        _ => return Err("unsupported ISO BMFF movie or media header version".into()),
    };
    let timescale = u32::from_be_bytes(
        payload
            .get(time_offset..time_offset + 4)
            .ok_or("truncated ISO BMFF timescale")?
            .try_into()
            .unwrap(),
    );
    let duration = payload
        .get(duration_offset..duration_offset + duration_size)
        .ok_or("truncated ISO BMFF header duration")?;
    let duration = if duration_size == 4 {
        u64::from(u32::from_be_bytes(duration.try_into().unwrap()))
    } else {
        u64::from_be_bytes(duration.try_into().unwrap())
    };
    if timescale == 0 || duration == 0 {
        return Err("ISO BMFF header has no usable timescale or duration".into());
    }
    Ok((timescale, duration))
}

fn edit_entry(payload: &[u8]) -> Result<(u64, u64), String> {
    let version = *payload.first().ok_or("truncated ISO BMFF edit list")?;
    let count = u32::from_be_bytes(
        payload
            .get(4..8)
            .ok_or("truncated ISO BMFF edit count")?
            .try_into()
            .unwrap(),
    );
    if count != 1 {
        return Err("ISO BMFF multi-entry edit lists are unsupported".into());
    }
    let (duration, media_time, rate) = match version {
        0 => {
            let entry = payload.get(8..20).ok_or("truncated ISO BMFF edit entry")?;
            (
                u64::from(u32::from_be_bytes(entry[..4].try_into().unwrap())),
                i64::from(i32::from_be_bytes(entry[4..8].try_into().unwrap())),
                &entry[8..12],
            )
        }
        1 => {
            let entry = payload.get(8..28).ok_or("truncated ISO BMFF edit entry")?;
            (
                u64::from_be_bytes(entry[..8].try_into().unwrap()),
                i64::from_be_bytes(entry[8..16].try_into().unwrap()),
                &entry[16..20],
            )
        }
        _ => return Err("unsupported ISO BMFF edit-list version".into()),
    };
    if duration == 0 || media_time < 0 || rate != [0, 1, 0, 0] {
        return Err("ISO BMFF edit is empty or has unsupported playback rate".into());
    }
    Ok((duration, media_time as u64))
}

#[cfg(test)]
mod tests;
