use super::*;

fn read(url: &str) -> LayoutRead {
    LayoutRead::DecodedSize(Rc::from(url), None)
}

#[test]
fn a_closed_frame_leaves_only_its_distinct_reads_to_the_enclosing_frame() {
    let mut frames = ReadFrames::default();
    frames.frames.push(OpenFrame {
        start: 0,
        retained_items: 0,
        depth: 0,
        shallowest: usize::MAX,
    });
    frames.record(read("before"));
    for url in ["a", "b", "a", "a"] {
        frames.record(read(url));
    }
    frames.compact(1..5, &[read("a"), read("b")]);
    frames.record(read("after"));
    assert_eq!(
        frames.log,
        [read("before"), read("a"), read("b"), read("after")]
    );
}

#[test]
fn an_outermost_frame_needs_no_compacting() {
    let mut frames = ReadFrames {
        log: vec![read("a"), read("a")],
        frames: Vec::new(),
    };
    frames.compact(0..2, &[read("a")]);
    assert_eq!(frames.log.len(), 2);
}
