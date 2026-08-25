use super::*;
use crate::renderer_protocol::DocumentId;

#[test]
fn closing_receiver_releases_fetch_backpressure() {
    let (sender, receiver) = bounded();
    let document = DocumentId::new(1).unwrap();
    sender
        .send(RendererEvent::FetchBatch {
            document,
            requests: Vec::new(),
        })
        .unwrap();
    let producer = std::thread::spawn(move || {
        sender.send(RendererEvent::FetchBatch {
            document,
            requests: Vec::new(),
        })
    });
    drop(receiver);
    producer.join().unwrap().unwrap();
}
