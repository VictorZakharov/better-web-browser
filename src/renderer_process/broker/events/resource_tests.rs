use super::*;
use crate::engine::DecodedImage;
use crate::limits::MAX_PRESENTED_IMAGES;
use crate::renderer_protocol::PresentedImage;

fn image(url: String, pixels: std::sync::Arc<[u8]>) -> PresentedImage {
    let size = if pixels.len() == 4 { 1 } else { 512 };
    PresentedImage {
        url,
        image: DecodedImage {
            width: size,
            height: size,
            bgra: pixels,
        },
    }
}

fn with_images(
    revision: u64,
    start: usize,
    count: usize,
    pixels: &std::sync::Arc<[u8]>,
) -> RendererEvent {
    let RendererEvent::Presentation(mut value) = presentation(revision) else {
        unreachable!()
    };
    value.glyph_epoch = 1;
    value.images = (start..start + count)
        .map(|id| image(format!("canvas:{id}"), pixels.clone()))
        .collect();
    RendererEvent::Presentation(value)
}

#[test]
fn canvas_bursts_bound_queued_bitmaps_and_keep_the_latest_pixels() {
    let (sender, receiver) = bounded();
    for revision in 1..50 {
        let pixels = vec![revision as u8; 4].into();
        sender.send(with_images(revision, 0, 1, &pixels)).unwrap();
    }
    assert_eq!(receiver.pending(), 1);
    let RendererEvent::Presentation(value) = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(value.revision, 49);
    assert_eq!(value.images.len(), 1);
    assert_eq!(value.images[0].image.bgra[0], 49);
}

#[test]
fn split_resource_chunks_survive_later_compaction_in_fifo_order() {
    let (sender, receiver) = bounded();
    let pixels = vec![1; 4].into();
    sender
        .send(with_images(1, 0, MAX_PRESENTED_IMAGES, &pixels))
        .unwrap();
    sender
        .send(with_images(2, MAX_PRESENTED_IMAGES, 1, &pixels))
        .unwrap();
    // This update cancels the second chunk's old bitmap, not the first chunk's resources.
    sender
        .send(with_images(3, MAX_PRESENTED_IMAGES, 1, &vec![3; 4].into()))
        .unwrap();
    assert_eq!(receiver.pending(), 2);
    let RendererEvent::Presentation(first) = receiver.try_recv().unwrap() else {
        panic!()
    };
    let RendererEvent::Presentation(next) = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(first.revision, 1);
    assert_eq!(first.images.len(), MAX_PRESENTED_IMAGES);
    assert_eq!(next.revision, 3);
    assert_eq!(next.images.len(), 1);
    assert_eq!(next.images[0].image.bgra[0], 3);
}

#[test]
fn disjoint_bitmap_byte_pressure_waits_for_browser_drain_instead_of_failing() {
    let (sender, receiver) = bounded();
    let pixels = vec![255; 1024 * 1024].into();
    sender.send(with_images(1, 0, 33, &pixels)).unwrap();
    let (done, completion) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        done.send(sender.send(with_images(2, 33, 33, &pixels)))
            .unwrap();
    });
    assert!(matches!(
        completion.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let RendererEvent::Presentation(first) = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(first.revision, 1);
    assert_eq!(first.images.len(), 33);
    completion
        .recv_timeout(Duration::from_secs(1))
        .expect("producer remained blocked")
        .unwrap();
    let RendererEvent::Presentation(next) = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(next.revision, 2);
    assert_eq!(next.images.len(), 33);
    producer.join().unwrap();
}

#[test]
fn receiver_close_releases_resource_budget_backpressure() {
    let (sender, receiver) = bounded();
    let pixels = vec![255; 1024 * 1024].into();
    sender.send(with_images(1, 0, 33, &pixels)).unwrap();
    let (done, completion) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        done.send(sender.send(with_images(2, 33, 33, &pixels)))
            .unwrap();
    });
    assert!(matches!(
        completion.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    receiver.close();
    completion
        .recv_timeout(Duration::from_secs(1))
        .expect("producer remained blocked after close")
        .unwrap();
    producer.join().unwrap();
    assert_eq!(receiver.pending(), 0);
}

#[test]
fn runtime_count_split_keeps_order_and_only_compacts_the_latest_chunk() {
    let (sender, receiver) = bounded();
    let RendererEvent::RuntimeUpdate(mut first) = runtime_update(1, "first") else {
        panic!()
    };
    first.runtime.console = vec!["first".into(); crate::limits::MAX_RUNTIME_REPORT_ENTRIES];
    sender.send(RendererEvent::RuntimeUpdate(first)).unwrap();
    sender.send(runtime_update(2, "second")).unwrap();
    sender.send(runtime_update(3, "third")).unwrap();
    assert_eq!(receiver.pending(), 2);
    let RendererEvent::RuntimeUpdate(first) = receiver.try_recv().unwrap() else {
        panic!()
    };
    let RendererEvent::RuntimeUpdate(next) = receiver.try_recv().unwrap() else {
        panic!()
    };
    assert_eq!(first.runtime.scripts_executed, 1);
    assert_eq!(
        first.runtime.console.len(),
        crate::limits::MAX_RUNTIME_REPORT_ENTRIES
    );
    assert_eq!(next.runtime.scripts_executed, 5);
    assert_eq!(next.runtime.console, ["second", "third"]);
}

#[test]
fn runtime_control_byte_limit_splits_without_dropping_entries() {
    let (sender, receiver) = bounded();
    for text in ["a", "b"] {
        let RendererEvent::RuntimeUpdate(mut value) = runtime_update(1, text) else {
            panic!()
        };
        value.runtime.console = vec![text.repeat(64 * 1024); 3];
        sender.send(RendererEvent::RuntimeUpdate(value)).unwrap();
    }
    assert_eq!(receiver.pending(), 2);
    for text in ["a", "b"] {
        let RendererEvent::RuntimeUpdate(value) = receiver.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(value.runtime.console, vec![text.repeat(64 * 1024); 3]);
    }
}
