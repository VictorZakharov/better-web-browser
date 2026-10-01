//! Container output for the same predictive Opus encoder and admission budget.

use crate::webm_opus::mux::Mux;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};

pub(super) enum Output {
    Ogg(PacketWriter<Vec<u8>>),
    Webm(Option<Mux>),
}

impl Output {
    pub(super) fn new(webm: bool) -> Self {
        if webm {
            Self::Webm(None)
        } else {
            Self::Ogg(PacketWriter::new(Vec::new()))
        }
    }

    pub(super) fn write_packet(
        &mut self,
        data: Box<[u8]>,
        serial: u32,
        ending: PacketWriteEndInfo,
        granule: u64,
    ) -> Result<(), &'static str> {
        match self {
            Self::Ogg(writer) => writer
                .write_packet(data, serial, ending, granule)
                .map_err(|_| "Could not write Ogg Opus packet"),
            Self::Webm(mux) => {
                if mux.is_none() {
                    *mux = Some(Mux::new(&data, serial)?);
                    return Ok(());
                }
                if data.starts_with(b"OpusTags") && granule == 0 {
                    return Ok(());
                }
                let final_granule =
                    matches!(ending, PacketWriteEndInfo::EndStream).then_some(granule);
                mux.as_mut().unwrap().packet(&data, final_granule)
            }
        }
    }

    pub(super) fn inner(&self) -> &[u8] {
        match self {
            Self::Ogg(writer) => writer.inner(),
            Self::Webm(mux) => mux.as_ref().unwrap().bytes(),
        }
    }

    pub(super) fn drain(&mut self) -> Result<Vec<u8>, &'static str> {
        match self {
            Self::Ogg(writer) => Ok(std::mem::take(writer.inner_mut())),
            Self::Webm(mux) => mux.as_mut().unwrap().drain(),
        }
    }
}
