use super::*;

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
    let wait = remaining(start + ms(100), frame, Some(start + ms(90)), PARSED);
    assert_eq!(wait, Some(ms(240)));
    // The input stalled for 250 ms.
    assert_eq!(
        remaining(start + ms(400), frame, Some(start + ms(150)), PARSED),
        None
    );
    // Input never stalls, but a second passed since the last frame.
    assert_eq!(
        remaining(start + ms(1000), frame, Some(start + ms(990)), PARSED),
        None
    );
    assert_eq!(
        remaining(start + ms(900), frame, Some(start + ms(890)), PARSED),
        Some(ms(100))
    );
}

#[test]
fn arrived_resources_wait_only_for_frame_spacing_even_while_streaming() {
    let start = Instant::now();
    let frame = (start, ms(100));
    // Twice the last frame's 100 ms cost: images show after 200 ms, not after parsing.
    for streaming in [Some(start + ms(140)), None] {
        assert_eq!(
            remaining(start + ms(150), frame, streaming, IMAGES),
            Some(ms(50))
        );
        assert_eq!(remaining(start + ms(200), frame, streaming, IMAGES), None);
    }
    // Cheap frames are still spaced by at least 50 ms.
    assert_eq!(
        remaining(start + ms(10), (start, ms(1)), None, IMAGES),
        Some(ms(40))
    );
}

#[test]
fn frames_after_parsing_with_other_work_do_not_wait() {
    let start = Instant::now();
    let frame = (start, ms(100));
    assert_eq!(
        remaining(start + ms(10), frame, None, IMAGES_AND_SCRIPT),
        None
    );
    assert_eq!(remaining(start + ms(10), frame, None, PARSED), None);
}
