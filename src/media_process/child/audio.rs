use super::super::backend::AudioDecoder;
use crate::media_protocol::{MediaDecodeReport, MediaPlaybackState};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

mod output;
use output::AudioOutput;

const CLOCK_TICK: Duration = Duration::from_millis(5);

type StateReply = SyncSender<Result<MediaPlaybackState, String>>;

enum AudioCommand {
    SetPlayback {
        playing: bool,
        volume_millis: u16,
        reply: StateReply,
    },
    State(StateReply),
    Shutdown,
}

pub(super) struct AudioPlayback {
    commands: SyncSender<AudioCommand>,
    thread: Option<JoinHandle<()>>,
}

impl AudioPlayback {
    pub(super) fn spawn(
        source_id: u64,
        bytes: Vec<u8>,
        report: MediaDecodeReport,
        test_mode: bool,
    ) -> Result<Self, String> {
        let (commands, receiver) = mpsc::sync_channel(4);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("breeze-media-audio".into())
            .spawn(move || {
                let runtime = AudioRuntime::new(source_id, &bytes, report, test_mode);
                match runtime {
                    Ok(runtime) => {
                        let _ = ready_tx.send(Ok(()));
                        runtime.run(receiver);
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })
            .map_err(|error| format!("start media audio thread: {error}"))?;
        match ready_rx.recv_timeout(crate::limits::MEDIA_COMMAND_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                commands,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(error) => Err(format!("media audio startup timed out: {error}")),
        }
    }

    pub(super) fn set_playback(
        &self,
        playing: bool,
        volume_millis: u16,
    ) -> Result<MediaPlaybackState, String> {
        self.request(|reply| AudioCommand::SetPlayback {
            playing,
            volume_millis,
            reply,
        })
    }

    pub(super) fn state(&self) -> Result<MediaPlaybackState, String> {
        self.request(AudioCommand::State)
    }

    fn request(
        &self,
        command: impl FnOnce(StateReply) -> AudioCommand,
    ) -> Result<MediaPlaybackState, String> {
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .send(command(reply))
            .map_err(|_| "media audio thread disconnected".to_string())?;
        result
            .recv_timeout(crate::limits::MEDIA_COMMAND_TIMEOUT)
            .map_err(|error| format!("media audio command timed out: {error}"))?
    }
}

impl Drop for AudioPlayback {
    fn drop(&mut self) {
        let _ = self.commands.send(AudioCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct AudioRuntime {
    source_id: u64,
    duration_100ns: u64,
    start_100ns: u64,
    decoder: AudioDecoder,
    output: AudioOutput,
}

impl AudioRuntime {
    fn new(
        source_id: u64,
        bytes: &[u8],
        report: MediaDecodeReport,
        test_mode: bool,
    ) -> Result<Self, String> {
        let decoder = AudioDecoder::open(
            bytes,
            report.audio_samples,
            report.audio_sample_rate,
            report.audio_channels,
        )?;
        let output = if test_mode {
            AudioOutput::silent()
        } else {
            AudioOutput::device(decoder.sample_rate(), decoder.channels())?
        };
        Ok(Self {
            source_id,
            duration_100ns: report.duration_100ns,
            start_100ns: report.audio_first_timestamp_100ns.max(0) as u64,
            decoder,
            output,
        })
    }

    fn run(mut self, receiver: Receiver<AudioCommand>) {
        loop {
            let command = if self.output.playing() {
                match receiver.recv_timeout(CLOCK_TICK) {
                    Ok(command) => Some(command),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match receiver.recv() {
                    Ok(command) => Some(command),
                    Err(_) => break,
                }
            };
            let Some(command) = command else {
                let _ = self.state();
                continue;
            };
            match command {
                AudioCommand::SetPlayback {
                    playing,
                    volume_millis,
                    reply,
                } => {
                    let result = self
                        .output
                        .set_playback(playing, volume_millis, &mut self.decoder)
                        .and_then(|_| self.state());
                    let _ = reply.send(result);
                }
                AudioCommand::State(reply) => {
                    let result = self.state();
                    let _ = reply.send(result);
                }
                AudioCommand::Shutdown => break,
            }
        }
    }

    fn state(&mut self) -> Result<MediaPlaybackState, String> {
        let output = self.output.state(&mut self.decoder)?;
        let position = self
            .start_100ns
            .saturating_add(output.position_100ns)
            .min(self.duration_100ns);
        let ended = output.ended || position >= self.duration_100ns;
        Ok(MediaPlaybackState {
            source_id: self.source_id,
            position_100ns: if ended { self.duration_100ns } else { position },
            duration_100ns: self.duration_100ns,
            playing: output.playing && !ended,
            ended,
        })
    }
}
