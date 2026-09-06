use super::super::backend::AudioDecoder;
use crate::media_protocol::{MediaDecodeReport, MediaPlaybackState};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

mod output;
use output::AudioOutput;

const CLOCK_TICK: Duration = Duration::from_millis(5);

type StateReply = SyncSender<Result<MediaPlaybackState, String>>;
type AppendReply = SyncSender<Result<(), String>>;

enum AudioCommand {
    SetPlayback {
        playing: bool,
        volume_millis: u16,
        reply: StateReply,
    },
    Seek {
        position_100ns: u64,
        reply: StateReply,
    },
    State(StateReply),
    Append {
        bytes: Vec<u8>,
        report: MediaDecodeReport,
        reply: AppendReply,
    },
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
        silent_audio: bool,
    ) -> Result<Self, String> {
        let (commands, receiver) = mpsc::sync_channel(4);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("breeze-media-audio".into())
            .spawn(move || {
                let runtime = AudioRuntime::new(source_id, &bytes, report, silent_audio);
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

    pub(super) fn seek(&self, position_100ns: u64) -> Result<MediaPlaybackState, String> {
        self.request(|reply| AudioCommand::Seek {
            position_100ns,
            reply,
        })
    }

    pub(super) fn append(&self, bytes: Vec<u8>, report: MediaDecodeReport) -> Result<(), String> {
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .send(AudioCommand::Append {
                bytes,
                report,
                reply,
            })
            .map_err(|_| "media audio thread disconnected".to_string())?;
        result
            .recv_timeout(crate::limits::MEDIA_COMMAND_TIMEOUT)
            .map_err(|error| format!("media audio append timed out: {error}"))?
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
    decoder: AudioDecoderQueue,
    output: AudioOutput,
}

impl AudioRuntime {
    fn new(
        source_id: u64,
        bytes: &[u8],
        report: MediaDecodeReport,
        silent_audio: bool,
    ) -> Result<Self, String> {
        let decoder = AudioDecoderQueue::new(bytes, report)?;
        let output = if silent_audio {
            AudioOutput::silent()
        } else {
            AudioOutput::device_or_silent(decoder.sample_rate(), decoder.channels())?
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
                AudioCommand::Seek {
                    position_100ns,
                    reply,
                } => {
                    let result = self.seek(position_100ns).and_then(|_| self.state());
                    let _ = reply.send(result);
                }
                AudioCommand::Append {
                    bytes,
                    report,
                    reply,
                } => {
                    let result = self.decoder.append(&bytes, report).map(|_| {
                        self.duration_100ns = self.duration_100ns.max(report.duration_100ns);
                        self.output.input_appended();
                    });
                    let _ = reply.send(result);
                }
                AudioCommand::Shutdown => break,
            }
        }
    }

    fn state(&mut self) -> Result<MediaPlaybackState, String> {
        let output = self.output.state(&mut self.decoder)?;
        Ok(playback_state(
            self.source_id,
            self.start_100ns,
            self.duration_100ns,
            output,
        ))
    }

    fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        let position_100ns = position_100ns.min(self.duration_100ns);
        self.decoder.seek(position_100ns)?;
        self.output.seek(
            position_100ns.saturating_sub(self.start_100ns),
            &mut self.decoder,
        )
    }
}

fn playback_state(
    source_id: u64,
    start_100ns: u64,
    duration_100ns: u64,
    output: output::OutputState,
) -> MediaPlaybackState {
    let position = start_100ns
        .saturating_add(output.position_100ns)
        .min(duration_100ns);
    // Exhausting the currently appended decoder input is MSE starvation, not
    // end-of-media, while the declared presentation duration remains ahead.
    let ended = position >= duration_100ns;
    MediaPlaybackState {
        source_id,
        position_100ns: if ended { duration_100ns } else { position },
        duration_100ns,
        playing: output.playing && !ended,
        ended,
    }
}

struct AudioDecoderQueue {
    segments: Vec<AudioDecoderSegment>,
    current: usize,
    sample_rate: u32,
    channels: u16,
}

struct AudioDecoderSegment {
    decoder: AudioDecoder,
    start_100ns: u64,
    end_100ns: u64,
}

impl AudioDecoderQueue {
    fn new(bytes: &[u8], report: MediaDecodeReport) -> Result<Self, String> {
        let mut queue = Self {
            segments: Vec::new(),
            current: 0,
            sample_rate: report.audio_sample_rate,
            channels: report.audio_channels,
        };
        queue.append(bytes, report)?;
        Ok(queue)
    }

    fn append(&mut self, bytes: &[u8], report: MediaDecodeReport) -> Result<(), String> {
        if report.audio_sample_rate != self.sample_rate || report.audio_channels != self.channels {
            return Err("incremental AAC format changed".into());
        }
        let decoder = AudioDecoder::open(
            bytes,
            report.audio_samples,
            report.audio_sample_rate,
            report.audio_channels,
        )?;
        self.segments.push(AudioDecoderSegment {
            decoder,
            start_100ns: report.audio_first_timestamp_100ns.max(0) as u64,
            end_100ns: report.duration_100ns,
        });
        Ok(())
    }

    const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    const fn channels(&self) -> u16 {
        self.channels
    }

    fn next_sample(&mut self) -> Result<Option<Vec<u8>>, String> {
        while let Some(segment) = self.segments.get_mut(self.current) {
            if let Some(sample) = segment.decoder.next_sample()? {
                return Ok(Some(sample));
            }
            self.current += 1;
        }
        Ok(None)
    }

    fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        let index = self
            .segments
            .iter()
            .position(|segment| {
                position_100ns >= segment.start_100ns && position_100ns < segment.end_100ns
            })
            .unwrap_or_else(|| self.segments.len().saturating_sub(1));
        self.current = index;
        self.segments[index].decoder.seek(position_100ns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_media_underflow_does_not_end_the_declared_presentation() {
        let starved = playback_state(
            3,
            0,
            600_000_000,
            output::OutputState {
                position_100ns: 200_000_000,
                playing: true,
            },
        );
        assert!(starved.playing);
        assert!(!starved.ended);

        let finished = playback_state(
            3,
            0,
            600_000_000,
            output::OutputState {
                position_100ns: 600_000_000,
                playing: true,
            },
        );
        assert!(!finished.playing);
        assert!(finished.ended);
    }
}
