use super::*;

impl ChildConnection {
    pub(in crate::renderer_process::child) fn media(
        &mut self,
    ) -> Option<&mut crate::media_process::MediaClient> {
        self.media.as_mut()
    }

    pub(in crate::renderer_process::child) fn decode_media(
        &mut self,
        bytes: &[u8],
    ) -> Result<crate::media_process::RendererMediaDecode, String> {
        let (media, writer, last_ack) = (
            &mut self.media,
            &mut self.writer,
            &mut self.last_processed_work_ack,
        );
        let media = media
            .as_mut()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?;
        // This wait has the media protocol's independent hard deadline, so progress
        // acknowledgements keep the renderer responsive without masking a hung worker.
        media.decode(bytes, || acknowledge_renderer_progress(writer, last_ack))
    }

    pub(in crate::renderer_process::child) fn decode_media_tracks(
        &mut self,
        video_bytes: &[u8],
        audio_bytes: &[u8],
    ) -> Result<crate::media_process::RendererMediaDecode, String> {
        let (media, writer, last_ack) = (
            &mut self.media,
            &mut self.writer,
            &mut self.last_processed_work_ack,
        );
        let media = media
            .as_mut()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?;
        media.decode_tracks(video_bytes, audio_bytes, || {
            acknowledge_renderer_progress(writer, last_ack)
        })
    }
}

pub(super) fn acknowledge_renderer_progress(
    writer: &mut FrameWriter<File>,
    last_ack: &mut Instant,
) -> Result<(), String> {
    if last_ack.elapsed() < PROCESSED_WORK_ACK_INTERVAL {
        return Ok(());
    }
    writer
        .send_renderer(&RendererMessage::Pong(0))
        .map_err(|error| error.to_string())?;
    *last_ack = Instant::now();
    Ok(())
}
