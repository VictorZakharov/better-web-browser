use super::*;
use crate::renderer_protocol::{SensorEvent, SensorReading, SensorUpdate};

fn update(document: DocumentId, request_id: u64, event: SensorEvent) -> SensorUpdate {
    SensorUpdate {
        document,
        request_id,
        event,
    }
}

#[test]
fn sensor_activation_precedes_samples_in_one_fifo_mailbox() {
    let document = DocumentId::new(9).unwrap();
    let (sender, receiver) = mpsc::sync_channel(2);
    let overflow = Arc::new(AtomicBool::new(false));
    let sink = SensorUpdateSink::new(
        document,
        sender,
        overflow,
        super::super::wake::BrokerWake::default(),
    );
    sink.try_send(update(document, 1, SensorEvent::Activated))
        .unwrap();
    sink.try_send(update(
        document,
        1,
        SensorEvent::Reading(SensorReading::ThreeAxis {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            timestamp_ms: 4.0,
        }),
    ))
    .unwrap();
    assert!(matches!(
        receiver.try_recv().unwrap().event,
        SensorEvent::Activated
    ));
    assert!(matches!(
        receiver.try_recv().unwrap().event,
        SensorEvent::Reading(_)
    ));
}

#[test]
fn only_saturated_sensor_control_updates_fail_the_session() {
    let document = DocumentId::new(9).unwrap();
    let (sender, _receiver) = mpsc::sync_channel(1);
    let overflow = Arc::new(AtomicBool::new(false));
    let sink = SensorUpdateSink::new(
        document,
        sender,
        Arc::clone(&overflow),
        super::super::wake::BrokerWake::default(),
    );
    sink.try_send(update(document, 1, SensorEvent::Activated))
        .unwrap();
    assert!(
        sink.try_send(update(
            document,
            1,
            SensorEvent::Reading(SensorReading::ThreeAxis {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                timestamp_ms: 4.0,
            })
        ))
        .is_err()
    );
    assert!(!overflow.load(Ordering::Acquire));
    assert!(
        sink.try_send(update(
            document,
            2,
            SensorEvent::Error(crate::renderer_protocol::SensorError::NotAllowed)
        ))
        .is_err()
    );
    assert!(overflow.load(Ordering::Acquire));
}
