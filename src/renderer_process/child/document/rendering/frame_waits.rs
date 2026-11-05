//! Skipped rendering opportunities while a document streams in and resources arrive.
//
// HTML lets a user agent skip rendering opportunities. Each frame of a long, growing document
// restyles and lays out the parsed content again: presenting after every network chunk and
// image of a large article made parsing take 5 s instead of 0.5 s, and its images arrived over
// seconds. After the first frame of a document whose input is still streaming in, the next
// checkpoint frame therefore waits until the input ends, stalls for `PARSER_STALL`, or
// `MAX_PARSER_FRAME_DELAY` has passed; WebKit likewise delays layouts while parsing. A parser
// waiting for a blocking script makes no progress, so its frame does not wait (the caller
// passes no streaming input then). Arrived resources need a frame sooner: such a checkpoint,
// and after parsing one that only installs resources, waits until `FRAME_SPACING` times the
// last frame's style and layout time has passed, though not ahead of input that keeps flowing:
// a document's bytes are cheap to parse and the page's scripts wait for them. Until the load
// event, script tasks follow each other closely, and laying out after every one of them
// (jQuery's ready chain alone is dozens of timers) takes far longer than the tasks do, so a frame
// after script work waits for as long as the last frame took. Once loaded, script-driven frames
// never wait, so animations keep their rate. Input and viewport changes present through their
// own paths and never wait.
// https://html.spec.whatwg.org/multipage/webappapis.html#update-the-rendering
use std::time::{Duration, Instant};

const FRAME_SPACING: u32 = 2;
const MIN_FRAME_SPACING: Duration = Duration::from_millis(50);
const PARSER_STALL: Duration = Duration::from_millis(250);
const MAX_PARSER_FRAME_DELAY: Duration = Duration::from_secs(1);
const RESOURCE_INPUT_QUIET: Duration = Duration::from_millis(50);
const MIN_LOADING_FRAME_SPACING: Duration = Duration::from_millis(16);

/// What a rendering checkpoint would present.
#[derive(Clone, Copy)]
pub(in crate::renderer_process::child::document) struct FrameArrivals {
    /// Images, fonts or stylesheets arrived since the last frame.
    pub(in crate::renderer_process::child::document) resources: bool,
    /// Nothing else needs a frame: no script, media, scroll or observer work.
    pub(in crate::renderer_process::child::document) only: bool,
}

/// How far the document's load has come.
#[derive(Clone, Copy)]
pub(super) enum LoadPhase {
    /// The parser last received streamed input at this time, and more is expected.
    Streaming(Instant),
    /// The load event has not fired yet.
    Loading,
    Loaded,
}

/// How much longer a checkpoint at `now` skips its frame, given when the last frame was
/// presented and what its style and layout cost. `None` presents now.
pub(super) fn remaining(
    now: Instant,
    (presented, cost): (Instant, Duration),
    load: LoadPhase,
    arrivals: FrameArrivals,
) -> Option<Duration> {
    let remaining = |wait: Duration, since: Instant| {
        wait.checked_sub(now.saturating_duration_since(since))
            .filter(|remaining| !remaining.is_zero())
    };
    let spacing = remaining((cost * FRAME_SPACING).max(MIN_FRAME_SPACING), presented);
    match load {
        // Arrived images and fonts show up as soon as the spacing allows, not only after
        // the whole document has streamed in.
        LoadPhase::Streaming(input) if arrivals.resources => {
            let flowing = remaining(MAX_PARSER_FRAME_DELAY, presented)
                .and(remaining(RESOURCE_INPUT_QUIET, input));
            spacing.max(flowing)
        }
        LoadPhase::Streaming(input) => {
            Some(remaining(MAX_PARSER_FRAME_DELAY, presented)?.min(remaining(PARSER_STALL, input)?))
        }
        _ if arrivals.resources && arrivals.only => spacing,
        LoadPhase::Loading => remaining(cost.max(MIN_LOADING_FRAME_SPACING), presented),
        LoadPhase::Loaded => None,
    }
}

#[cfg(test)]
mod tests;
