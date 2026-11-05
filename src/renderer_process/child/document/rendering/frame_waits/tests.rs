use super::*;
use LoadPhase::{Loaded, Loading, Streaming};

const PARSED: FrameArrivals = FrameArrivals {
    resources: false,
    only: false,
};
const IMAGES: FrameArrivals = FrameArrivals {
    resources: true,
    only: true,
};
const IMAGES_AND_SCRIPT: FrameArrivals = FrameArrivals {
    resources: true,
    only: false,
};

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn streaming_content_waits_for_a_stall_or_the_maximum_delay() {
    let start = Instant::now();
    let frame = (start, ms(100));
    // Input keeps arriving: the frame waits for the parser to stall.
    let wait = remaining(start + ms(100), frame, Streaming(start + ms(90)), PARSED);
    assert_eq!(wait, Some(ms(240)));
    // The input stalled for 250 ms.
    assert_eq!(
        remaining(start + ms(400), frame, Streaming(start + ms(150)), PARSED),
        None
    );
    // Input never stalls, but a second passed since the last frame.
    assert_eq!(
        remaining(start + ms(1000), frame, Streaming(start + ms(990)), PARSED),
        None
    );
    assert_eq!(
        remaining(start + ms(900), frame, Streaming(start + ms(890)), PARSED),
        Some(ms(100))
    );
}

#[test]
fn arrived_resources_wait_for_frame_spacing_at_every_load_phase() {
    let start = Instant::now();
    let frame = (start, ms(100));
    // Twice the last frame's 100 ms cost: images show after 200 ms, not after parsing.
    for load in [Streaming(start + ms(140)), Loading, Loaded] {
        assert_eq!(
            remaining(start + ms(190), frame, load, IMAGES),
            Some(ms(10))
        );
        assert_eq!(remaining(start + ms(200), frame, load, IMAGES), None);
    }
    // Cheap frames are still spaced by at least 50 ms.
    assert_eq!(
        remaining(start + ms(10), (start, ms(1)), Loaded, IMAGES),
        Some(ms(40))
    );
}

#[test]
fn arrived_resources_do_not_interrupt_flowing_input() {
    let start = Instant::now();
    let frame = (start, ms(100));
    // The spacing passed, but document bytes arrived 10 ms ago.
    assert_eq!(
        remaining(start + ms(300), frame, Streaming(start + ms(290)), IMAGES),
        Some(ms(40))
    );
    // The input has been quiet for 50 ms.
    assert_eq!(
        remaining(start + ms(300), frame, Streaming(start + ms(250)), IMAGES),
        None
    );
    // Input never goes quiet, but a second passed since the last frame.
    assert_eq!(
        remaining(start + ms(1000), frame, Streaming(start + ms(990)), IMAGES),
        None
    );
}

#[test]
fn script_frames_wait_for_the_last_frame_cost_only_while_loading() {
    let start = Instant::now();
    let frame = (start, ms(100));
    for arrivals in [PARSED, IMAGES_AND_SCRIPT] {
        assert_eq!(
            remaining(start + ms(60), frame, Loading, arrivals),
            Some(ms(40))
        );
        assert_eq!(remaining(start + ms(100), frame, Loading, arrivals), None);
        // A loaded page keeps its animation rate.
        assert_eq!(remaining(start + ms(10), frame, Loaded, arrivals), None);
    }
    // Frames cheaper than 50 ms never wait.
    assert_eq!(
        remaining(start + ms(1), (start, ms(49)), Loading, PARSED),
        None
    );
    assert_eq!(
        remaining(start + ms(1), (start, ms(50)), Loading, PARSED),
        Some(ms(49))
    );
}
