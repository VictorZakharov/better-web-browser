use super::*;

#[test]
fn bounded_wait_reports_progress_until_the_worker_replies() {
    let (sender, incoming) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(350));
        sender
            .send(Ok(WorkerMediaMessage::Pong(9)))
            .expect("deliver delayed media response");
    });
    let mut progress = 0;
    let response =
        receive_from_with_progress(&incoming, "test delay", Duration::from_secs(1), || {
            progress += 1;
            Ok(())
        })
        .expect("receive delayed media response");
    assert_eq!(response, WorkerMediaMessage::Pong(9));
    assert!(progress >= 2, "wait did not report bounded progress");
    worker.join().expect("join delayed media worker");
}
