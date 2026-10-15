use super::output::AudioOutput;
use crate::media_protocol::GraphPcmHealth;
use crate::media_protocol::{GraphPcmFormat, GraphPcmStatus};
use std::time::Instant;

/// A separate voice from decoded media. Only one graph stream is admitted per media worker.
pub(in crate::media_process::child) struct GraphAudio {
    active: Option<GraphStream>,
    silent_audio: bool,
}

struct GraphStream {
    document_id: u64,
    context_id: u64,
    format: GraphPcmFormat,
    output: AudioOutput,
    silent_started: Instant,
    silent_submitted_frames: u64,
}

impl GraphAudio {
    pub(in crate::media_process::child) fn new(silent_audio: bool) -> Self {
        Self {
            active: None,
            silent_audio,
        }
    }

    pub(in crate::media_process::child) fn queue(
        &mut self,
        document_id: u64,
        context_id: u64,
        format: GraphPcmFormat,
        pcm: Vec<u8>,
    ) -> Result<GraphPcmStatus, String> {
        format
            .validate(pcm.len())
            .map_err(|error| error.to_string())?;
        if self.active.is_none() {
            let output = if self.silent_audio {
                AudioOutput::silent()
            } else {
                AudioOutput::device_required(format.sample_rate, format.channels)?
            };
            self.active = Some(GraphStream {
                document_id,
                context_id,
                format,
                output,
                silent_started: Instant::now(),
                silent_submitted_frames: 0,
            });
        }
        let active = self.active.as_mut().expect("graph stream initialized");
        if active.document_id != document_id
            || active.context_id != context_id
            || active.format != format
        {
            return Ok(GraphPcmStatus::Rejected);
        }
        let frames = pcm.len() / usize::from(format.channels) / 2;
        let result = active.output.queue_graph_pcm(pcm, format.channels);
        if matches!(result, Ok(true)) {
            active.silent_submitted_frames =
                active.silent_submitted_frames.saturating_add(frames as u64);
        }
        if result.is_err() {
            self.active = None;
        }
        result.map(|accepted| {
            if accepted {
                GraphPcmStatus::Accepted
            } else {
                GraphPcmStatus::Backpressure
            }
        })
    }

    pub(in crate::media_process::child) fn health(
        &self,
        document_id: u64,
        context_id: u64,
    ) -> GraphPcmHealth {
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| active.document_id == document_id && active.context_id == context_id)
        else {
            return GraphPcmHealth::default();
        };
        active
            .output
            .graph_health()
            .unwrap_or_else(|| GraphPcmHealth {
                submitted_frames: active.silent_submitted_frames,
                played_frames: (active.silent_started.elapsed().as_secs_f64()
                    * f64::from(active.format.sample_rate))
                .min(active.silent_submitted_frames as f64) as u64,
                underruns: 0,
                playing: true,
            })
    }

    pub(in crate::media_process::child) fn close(
        &mut self,
        document_id: u64,
        context_id: u64,
    ) -> bool {
        if self.active.as_ref().is_some_and(|active| {
            active.document_id == document_id && active.context_id == context_id
        }) {
            self.active = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_stream_rejects_other_context_and_closes_by_identity() {
        let mut graph = GraphAudio::new(true);
        let format = GraphPcmFormat {
            sample_rate: 48_000,
            channels: 2,
        };
        assert_eq!(
            graph.queue(3, 5, format, vec![0; 512]).unwrap(),
            GraphPcmStatus::Accepted
        );
        assert_eq!(
            graph.queue(4, 5, format, vec![0; 512]).unwrap(),
            GraphPcmStatus::Rejected
        );
        assert_eq!(
            graph.queue(3, 6, format, vec![0; 512]).unwrap(),
            GraphPcmStatus::Rejected
        );
        assert!(!graph.close(4, 5));
        assert!(graph.close(3, 5));
        assert_eq!(
            graph.queue(4, 5, format, vec![0; 512]).unwrap(),
            GraphPcmStatus::Accepted
        );
    }
}
