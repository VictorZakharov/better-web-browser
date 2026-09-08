use super::*;
use crate::renderer_protocol::{DocumentId, VideoFrameIdentity, VideoFrameUpdate};

fn frame(number: u64) -> RendererEvent {
    RendererEvent::VideoFrame(Box::new(VideoFrameUpdate {
        identity: VideoFrameIdentity {
            document: DocumentId::new(1).unwrap(),
            revision: 2,
            frame: number,
            node: 1,
            width: 1,
            height: 1,
        },
        pixels: vec![0, 0, 0, 255].into(),
    }))
}

#[test]
fn slow_consumer_keeps_only_the_latest_video_frame() {
    let (sender, receiver) = bounded();
    for number in 1..=1000 {
        sender.send(frame(number)).unwrap();
    }
    assert_eq!(sender.pending(), 1);
    assert!(
        matches!(receiver.try_recv().unwrap(), RendererEvent::VideoFrame(update)
        if update.identity.frame == 1000)
    );
}

#[test]
fn navigation_discards_queued_video_pixels() {
    let (sender, receiver) = bounded();
    sender.send(frame(1)).unwrap();
    sender.discard_document(DocumentId::new(1).unwrap());
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}
