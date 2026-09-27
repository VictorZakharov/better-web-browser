use super::*;

#[test]
fn command_count_is_bounded_and_receiving_reopens_capacity() {
    let document_bytes = Arc::new(AtomicUsize::new(0));
    let (mailbox, receiver) = Mailbox::new(document_bytes.clone());
    for id in 0..MAX_PENDING_COMMANDS {
        mailbox
            .try_send(WorkerCommand::PortClose(id as u32))
            .unwrap();
    }
    assert_eq!(
        mailbox.try_send(WorkerCommand::PortClose(999)),
        Err(AdmissionError::Saturated)
    );
    let mut first = receiver.try_recv().unwrap();
    assert!(matches!(first.take(), WorkerCommand::PortClose(0)));
    // Receiving frees one queue slot; the byte reservation remains until the
    // Worker finishes processing the received command.
    mailbox.try_send(WorkerCommand::PortClose(999)).unwrap();
    assert_eq!(
        document_bytes.load(Ordering::Acquire),
        (MAX_PENDING_COMMANDS + 1) * COMMAND_OVERHEAD_BYTES
    );
    drop(first);
    assert_eq!(
        mailbox.try_send(WorkerCommand::PortClose(1000)),
        Err(AdmissionError::Saturated)
    );
    drop(receiver);
    assert_eq!(document_bytes.load(Ordering::Acquire), 0);
}

#[test]
fn byte_budget_counts_payloads_and_drops_release_every_reservation() {
    let document_bytes = Arc::new(AtomicUsize::new(0));
    let (mailbox, receiver) = Mailbox::new(document_bytes.clone());
    let payload = String::from_utf8(vec![
        b'x';
        MAX_WORKER_PENDING_BYTES - COMMAND_OVERHEAD_BYTES
    ])
    .unwrap();
    mailbox.try_send(WorkerCommand::Message(payload)).unwrap();
    assert_eq!(
        mailbox.try_send(WorkerCommand::PortClose(1)),
        Err(AdmissionError::Saturated)
    );
    assert_eq!(
        mailbox.worker_bytes.load(Ordering::Acquire),
        MAX_WORKER_PENDING_BYTES
    );
    drop(receiver);
    assert_eq!(mailbox.worker_bytes.load(Ordering::Acquire), 0);
    assert_eq!(document_bytes.load(Ordering::Acquire), 0);
}

#[test]
fn shared_document_budget_is_enforced_without_retaining_failed_command() {
    let preexisting = MAX_DOCUMENT_WORKER_PENDING_BYTES - COMMAND_OVERHEAD_BYTES;
    let document_bytes = Arc::new(AtomicUsize::new(preexisting));
    let (mailbox, receiver) = Mailbox::new(document_bytes.clone());
    mailbox.try_send(WorkerCommand::PortClose(1)).unwrap();
    assert_eq!(
        mailbox.try_send(WorkerCommand::PortClose(2)),
        Err(AdmissionError::Saturated)
    );
    drop(receiver);
    assert_eq!(document_bytes.load(Ordering::Acquire), preexisting);
}

#[test]
fn termination_never_waits_for_a_saturated_mailbox() {
    let document_bytes = Arc::new(AtomicUsize::new(0));
    let (commands, receiver) = Mailbox::new(document_bytes.clone());
    for id in 0..MAX_PENDING_COMMANDS {
        commands
            .try_send(WorkerCommand::PortClose(id as u32))
            .unwrap();
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    WorkerHandle {
        commands,
        cancelled: cancelled.clone(),
    }
    .terminate();
    assert!(cancelled.load(Ordering::Acquire));
    drop(receiver);
    assert_eq!(document_bytes.load(Ordering::Acquire), 0);
}

#[test]
fn disconnected_receiver_releases_failed_admission() {
    let document_bytes = Arc::new(AtomicUsize::new(0));
    let (mailbox, receiver) = Mailbox::new(document_bytes.clone());
    drop(receiver);
    assert_eq!(
        mailbox.try_send(WorkerCommand::Message("test".into())),
        Err(AdmissionError::Disconnected)
    );
    assert_eq!(document_bytes.load(Ordering::Acquire), 0);
}
