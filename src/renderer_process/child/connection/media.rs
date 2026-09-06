use super::*;
use std::sync::{Arc, Mutex, mpsc};
mod video;

pub(in crate::renderer_process::child) enum MediaOperationCompletion {
    Decoded(Result<crate::media_process::RendererMediaDecode, String>),
    Appended(Result<(u64, u64), String>),
}

pub(super) struct AsyncMediaClient {
    client: Arc<Mutex<crate::media_process::MediaClient>>,
    pending: Option<mpsc::Receiver<MediaOperationCompletion>>,
    video: video::VideoPump,
}

impl AsyncMediaClient {
    pub(super) fn new(
        client: crate::media_process::MediaClient,
        writer: super::writer::SharedWriter,
    ) -> Self {
        let client = Arc::new(Mutex::new(client));
        Self {
            video: video::VideoPump::new(Arc::clone(&client), writer),
            client,
            pending: None,
        }
    }

    fn start(
        &mut self,
        operation: impl FnOnce(&mut crate::media_process::MediaClient) -> MediaOperationCompletion
        + Send
        + 'static,
        failure: fn(String) -> MediaOperationCompletion,
    ) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("contained media worker already has an asynchronous operation".into());
        }
        let client = Arc::clone(&self.client);
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("breeze-renderer-media-operation".into())
            .spawn(move || {
                let completion = match client.lock() {
                    Ok(mut client) => operation(&mut client),
                    Err(_) => failure("contained media client lock was poisoned".into()),
                };
                let _ = sender.send(completion);
            })
            .map_err(|error| format!("start contained media operation: {error}"))?;
        self.pending = Some(receiver);
        Ok(())
    }

    fn poll(&mut self) -> Result<Option<MediaOperationCompletion>, String> {
        let Some(receiver) = self.pending.as_ref() else {
            return Ok(None);
        };
        match receiver.try_recv() {
            Ok(completion) => {
                self.pending = None;
                Ok(Some(completion))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                Err("contained media operation thread disconnected".into())
            }
        }
    }

    fn with_client<T>(
        &self,
        operation: impl FnOnce(&mut crate::media_process::MediaClient) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.pending.is_some() {
            return Err("contained media worker is busy".into());
        }
        let mut client = self
            .client
            .lock()
            .map_err(|_| "contained media client lock was poisoned".to_string())?;
        operation(&mut client)
    }
}

impl ChildConnection {
    pub(in crate::renderer_process::child) fn media_operation_pending(&self) -> bool {
        self.media
            .as_ref()
            .is_some_and(|media| media.pending.is_some())
    }

    pub(in crate::renderer_process::child) fn poll_media_operation(
        &mut self,
    ) -> Result<Option<MediaOperationCompletion>, String> {
        match self.media.as_mut() {
            Some(media) => media.poll(),
            None => Ok(None),
        }
    }

    pub(in crate::renderer_process::child) fn decode_media(
        &mut self,
        bytes: &[u8],
    ) -> Result<crate::media_process::RendererMediaDecode, String> {
        self.clear_video();
        let (media, writer, last_ack) = (
            &self.media,
            &mut self.writer,
            &mut self.last_processed_work_ack,
        );
        let media = media
            .as_ref()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?;
        media.with_client(|client| {
            // This wait has the media protocol's independent hard deadline, so progress
            // acknowledgements keep the renderer responsive without masking a hung worker.
            client.decode(bytes, || acknowledge_renderer_progress(writer, last_ack))
        })
    }

    pub(in crate::renderer_process::child) fn start_decode_media_tracks(
        &mut self,
        video_bytes: &[u8],
        audio_bytes: &[u8],
    ) -> Result<(), String> {
        self.clear_video();
        let media = self
            .media
            .as_mut()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?;
        let video_bytes = video_bytes.to_vec();
        let audio_bytes = audio_bytes.to_vec();
        media.start(
            move |client| {
                MediaOperationCompletion::Decoded(client.decode_tracks(
                    &video_bytes,
                    &audio_bytes,
                    || Ok(()),
                ))
            },
            |error| MediaOperationCompletion::Decoded(Err(error)),
        )
    }

    pub(in crate::renderer_process::child) fn start_append_media_tracks(
        &mut self,
        source_id: u64,
        video_bytes: &[u8],
        audio_bytes: &[u8],
    ) -> Result<(), String> {
        let media = self
            .media
            .as_mut()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?;
        let video_bytes = video_bytes.to_vec();
        let audio_bytes = audio_bytes.to_vec();
        media.start(
            move |client| {
                MediaOperationCompletion::Appended(client.append_tracks(
                    source_id,
                    &video_bytes,
                    &audio_bytes,
                    || Ok(()),
                ))
            },
            |error| MediaOperationCompletion::Appended(Err(error)),
        )
    }

    pub(in crate::renderer_process::child) fn next_media_frame(
        &self,
        source_id: u64,
    ) -> Result<Option<crate::media_process::DecodedMediaFrame>, String> {
        self.media
            .as_ref()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?
            .with_client(|media| media.next_frame(source_id))
    }

    pub(in crate::renderer_process::child) fn set_media_playback(
        &self,
        source_id: u64,
        playing: bool,
        volume_millis: u16,
    ) -> Result<crate::media_protocol::MediaPlaybackState, String> {
        let media = self
            .media
            .as_ref()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?;
        let state =
            media.with_client(|media| media.set_playback(source_id, playing, volume_millis))?;
        media.video.playback_changed(state);
        media.video.start()?;
        Ok(state)
    }

    pub(in crate::renderer_process::child) fn seek_media_playback(
        &self,
        source_id: u64,
        position_100ns: u64,
    ) -> Result<crate::media_protocol::MediaPlaybackState, String> {
        self.clear_video();
        self.media
            .as_ref()
            .ok_or_else(|| "contained media worker is unavailable".to_string())?
            .with_client(|media| media.seek_playback(source_id, position_100ns))
    }
}

pub(super) fn acknowledge_renderer_progress(
    writer: &mut super::writer::SharedWriter,
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
