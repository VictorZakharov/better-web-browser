//! Incremental audio-only WebM through the pinned upstream EBML writer.
//! Author-controlled EBML is never fed to this serializer. All tags are bounded
//! browser-generated metadata; libopus supplies the compressed packets.

use super::header;
use webm_iterable::matroska_spec::{Master, MatroskaSpec as Tag};
use webm_iterable::{WebmWriter, WriteOptions};

pub(crate) struct Mux {
    writer: WebmWriter<Vec<u8>>,
    raw_frames: u64,
    pre_skip: u16,
    finished: bool,
    failed: bool,
    total_bytes: usize,
}

impl Mux {
    pub(crate) fn new(head: &[u8], serial: u32) -> Result<Self, &'static str> {
        let header = header::read(head).map_err(|_| "WebM recorder identification is invalid")?;
        let mut writer = WebmWriter::new(Vec::new());
        writer
            .write(&Tag::Ebml(Master::Full(vec![
                Tag::EbmlVersion(1),
                Tag::EbmlReadVersion(1),
                Tag::EbmlMaxIdLength(4),
                Tag::EbmlMaxSizeLength(8),
                Tag::DocType("webm".into()),
                Tag::DocTypeVersion(4),
                Tag::DocTypeReadVersion(2),
            ])))
            .map_err(|_| "Could not write WebM EBML header")?;
        writer
            .write_advanced(
                &Tag::Segment(Master::Start),
                WriteOptions::is_unknown_sized_element(),
            )
            .map_err(|_| "Could not start WebM Segment")?;
        writer
            .write(&Tag::Info(Master::Full(vec![
                Tag::TimestampScale(1_000_000),
                Tag::MuxingApp("Breeze".into()),
                Tag::WritingApp("Breeze".into()),
            ])))
            .map_err(|_| "Could not write WebM timing metadata")?;
        writer
            .write(&Tag::Tracks(Master::Full(vec![Tag::TrackEntry(
                Master::Full(vec![
                    Tag::TrackNumber(1),
                    Tag::TrackUID(u64::from(serial).max(1)),
                    Tag::TrackType(2),
                    Tag::FlagLacing(0),
                    Tag::CodecID("A_OPUS".into()),
                    Tag::CodecPrivate(head.to_vec()),
                    Tag::CodecDelay(u64::from(header.pre_skip) * 1_000_000_000 / 48_000),
                    Tag::SeekPreRoll(80_000_000),
                    Tag::Audio(Master::Full(vec![
                        Tag::Channels(u64::from(header.channels)),
                        Tag::SamplingFrequency(f64::from(header.input_rate)),
                    ])),
                ]),
            )])))
            .map_err(|_| "Could not write WebM Opus track")?;
        Ok(Self {
            writer,
            raw_frames: 0,
            pre_skip: header.pre_skip,
            finished: false,
            failed: false,
            total_bytes: 0,
        })
    }

    pub(crate) fn packet(
        &mut self,
        packet: &[u8],
        final_granule: Option<u64>,
    ) -> Result<(), &'static str> {
        if self.finished || self.failed {
            return Err("WebM recorder is closed or failed");
        }
        let result = self.write_packet(packet, final_granule);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn write_packet(
        &mut self,
        packet: &[u8],
        final_granule: Option<u64>,
    ) -> Result<(), &'static str> {
        if packet.is_empty() || packet.len() > 4_000 {
            return Err("WebM recorder packet size is invalid");
        }
        let frames = opus::packet::get_nb_samples(packet, 48_000)
            .map_err(|_| "WebM recorder packet duration is invalid")? as u64;
        if frames != 960 {
            return Err("WebM recorder requires its 20ms encoder packets");
        }
        let end = self
            .raw_frames
            .checked_add(frames)
            .ok_or("WebM recorder timestamp overflows")?;
        if end > 48_000 * 3_600 {
            return Err("WebM recorder duration exceeds its bound");
        }
        let padding = match final_granule {
            Some(granule)
                if granule >= self.raw_frames
                    && granule <= end
                    && granule >= u64::from(self.pre_skip) =>
            {
                end - granule
            }
            Some(_) => return Err("WebM recorder final presentation endpoint is invalid"),
            None => 0,
        };
        let time_ms = self.raw_frames * 1_000 / 48_000;
        let mut block = vec![0x81, 0, 0, 0]; // Track 1, relative timestamp 0, no lacing.
        block.extend_from_slice(packet);
        let mut group = vec![Tag::Block(block)];
        if padding != 0 {
            group.push(Tag::DiscardPadding(
                (padding * 1_000_000_000 / 48_000) as i64,
            ));
        }
        // One bounded, finite Cluster per packet permits prompt Blob delivery;
        // the unknown Segment never buffers the entire recording in the writer.
        self.writer
            .write(&Tag::Cluster(Master::Full(vec![
                Tag::Timestamp(time_ms),
                Tag::BlockGroup(Master::Full(group)),
            ])))
            .map_err(|_| "Could not write WebM Opus packet")?;
        self.raw_frames = end;
        if final_granule.is_some() {
            self.writer
                .write(&Tag::Segment(Master::End))
                .map_err(|_| "Could not finalize WebM Segment")?;
            self.finished = true;
        }
        Ok(())
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.writer.get_ref()
    }
    pub(crate) fn drain(&mut self) -> Result<Vec<u8>, &'static str> {
        if self.failed {
            return Err("WebM recorder is failed");
        }
        let Some(total) = self
            .total_bytes
            .checked_add(self.bytes().len())
            .filter(|n| *n <= crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES)
        else {
            self.failed = true;
            return Err("WebM recorder exceeds its cumulative source limit");
        };
        self.total_bytes = total;
        let bytes = std::mem::take(self.writer.get_mut());
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests;
