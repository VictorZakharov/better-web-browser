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
// last frame's style and layout time has passed. Input and viewport changes present through
// their own paths and never wait.
// https://html.spec.whatwg.org/multipage/webappapis.html#update-the-rendering
use std::time::{Duration, Instant};

const FRAME_SPACING: u32 = 2;
const MIN_FRAME_SPACING: Duration = Duration::from_millis(50);
const PARSER_STALL: Duration = Duration::from_millis(250);
const MAX_PARSER_FRAME_DELAY: Duration = Duration::from_secs(1);

/// What a rendering checkpoint would present.
#[derive(Clone, Copy)]
pub(in crate::renderer_process::child::document) struct FrameArrivals {
    /// Images, fonts or stylesheets arrived since the last frame.
    pub(in crate::renderer_process::child::document) resources: bool,
    /// Nothing else needs a frame: no script, media, scroll or observer work.
    pub(in crate::renderer_process::child::document) only: bool,
}

/// How much longer a checkpoint at `now` skips its frame, given when the last frame was
/// presented and what its style and layout cost, and when streamed input last arrived while
/// more is expected. `None` presents now.
pub(super) fn remaining(
    now: Instant,
    (presented, cost): (Instant, Duration),
    streaming_input: Option<Instant>,
    arrivals: FrameArrivals,
) -> Option<Duration> {
    let remaining = |wait: Duration, since: Instant| {
        wait.checked_sub(now.saturating_duration_since(since))
            .filter(|remaining| !remaining.is_zero())
    };
    let spacing = remaining((cost * FRAME_SPACING).max(MIN_FRAME_SPACING), presented);
    match streaming_input {
        // Arrived images and fonts show up as soon as the spacing allows, not only after
        // the whole document has streamed in.
        Some(_) if arrivals.resources => spacing,
        Some(input) => {
            Some(remaining(MAX_PARSER_FRAME_DELAY, presented)?.min(remaining(PARSER_STALL, input)?))
        }
        None if arrivals.resources && arrivals.only => spacing,
        None => None,
    }
}

#[cfg(test)]
mod tests;
