use super::*;

// Four 896-frame stereo chunks, or about 75 ms at 48 kHz. The renderer may
// combine several 128-frame Web Audio quanta before submitting a chunk.
const MAX_GRAPH_PCM_QUEUED_BYTES: usize = 4 * crate::media_protocol::MAX_GRAPH_PCM_FRAMES * 2 * 2;

fn has_room(queued_buffers: usize, queued_bytes: usize, next_bytes: usize) -> bool {
    queued_buffers < QUEUED_AUDIO_SAMPLES
        && queued_bytes
            .checked_add(next_bytes)
            .is_some_and(|bytes| bytes <= MAX_GRAPH_PCM_QUEUED_BYTES)
}

impl AudioOutput {
    pub(in crate::media_process::child::audio) fn queue_graph_pcm(
        &mut self,
        bytes: Vec<u8>,
    ) -> Result<bool, String> {
        match self {
            Self::Silent(_) => Ok(true),
            Self::Device(output) => output.queue_graph_pcm(bytes),
        }
    }
}

impl XAudioOutput {
    fn queue_graph_pcm(&mut self, bytes: Vec<u8>) -> Result<bool, String> {
        let state = self.voice_state();
        while self.queued.len() > state.BuffersQueued as usize {
            if let Some(completed) = self.queued.pop_front() {
                self.queued_bytes = self.queued_bytes.saturating_sub(completed.len());
            }
        }
        if !has_room(self.queued.len(), self.queued_bytes, bytes.len()) {
            return Ok(false);
        }
        let next_bytes = self.queued_bytes + bytes.len();
        let buffer = XAUDIO2_BUFFER {
            AudioBytes: u32::try_from(bytes.len())
                .map_err(|_| "graph PCM chunk is too large".to_string())?,
            pAudioData: bytes.as_ptr(),
            ..Default::default()
        };
        unsafe { self.source.SubmitSourceBuffer(&buffer, None) }
            .map_err(|error| format!("queue XAudio2 graph PCM: {error}"))?;
        // XAudio2 borrows pAudioData until the buffer completes; the queue owns its bytes.
        self.queued.push_back(bytes);
        self.queued_bytes = next_bytes;
        if !self.playing {
            unsafe { self.source.Start(0, XAUDIO2_COMMIT_NOW) }
                .map_err(|error| format!("start XAudio2 graph PCM voice: {error}"))?;
            self.playing = true;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_queue_backpressures_at_four_chunks_and_recovers_after_underrun() {
        for queued in 0..QUEUED_AUDIO_SAMPLES {
            assert!(has_room(queued, queued * 3_584, 3_584));
        }
        assert!(!has_room(QUEUED_AUDIO_SAMPLES, 14_336, 3_584));
        // Completed buffers are retired using the voice's BuffersQueued count. When the voice
        // empties, the started voice can consume the next admitted chunk.
        assert!(has_room(0, 0, 3_584));
        assert!(!has_room(0, 14_336, 3_584));
        assert!(!has_room(0, usize::MAX, 3_584));
    }
}
