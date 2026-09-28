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
        receiver.try_recv().unwrap().update.event,
        SensorEvent::Activated
    ));
    assert!(matches!(
        receiver.try_recv().unwrap().update.event,
        SensorEvent::Reading(_)
    ));
}

#[test]
fn only_saturated_sensor_control_updates_fail_the_session() {
    let document = DocumentId::new(9).unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    let overflow = Arc::new(AtomicBool::new(false));
    let sink = SensorUpdateSink::new(
        document,
        sender,
        Arc::clone(&overflow),
        super::super::wake::BrokerWake::default(),
    );
    sink.try_send(update(document, 1, SensorEvent::Activated))
        .unwrap();
    assert_eq!(
        sink.try_send(update(
            document,
            1,
            SensorEvent::Reading(SensorReading::ThreeAxis {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                timestamp_ms: 4.0,
            })
        )),
        Err(SensorSinkError::Full)
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
    drop(receiver);
    assert_eq!(
        sink.try_send(update(
            document,
            1,
            SensorEvent::Reading(SensorReading::ThreeAxis {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                timestamp_ms: 5.0,
            })
        )),
        Err(SensorSinkError::Disconnected)
    );
}

#[test]
fn queued_reading_is_discarded_after_hide_even_if_tab_is_visible_again() {
    let document = DocumentId::new(10).unwrap();
    let tab = 7;
    let gate = SensorDeliveryGate::default();
    gate.set_visible_tab(tab);
    let (sender, receiver) = mpsc::sync_channel(2);
    let sink = SensorUpdateSink::new(
        document,
        sender,
        Arc::new(AtomicBool::new(false)),
        super::super::wake::BrokerWake::default(),
    )
    .with_delivery_gate(gate.clone(), tab);
    let reading = |timestamp_ms| {
        update(
            document,
            1,
            SensorEvent::Reading(SensorReading::ThreeAxis {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                timestamp_ms,
            }),
        )
    };
    sink.try_send(reading(1.0)).unwrap();
    gate.clear_visible_tab(tab);
    assert_eq!(sink.try_send(reading(2.0)), Err(SensorSinkError::Hidden));
    gate.set_visible_tab(tab);
    let stale = receiver.try_recv().unwrap();
    assert_eq!(
        stale.forward_if_current(|_| panic!("stale reading escaped")),
        Ok(false)
    );
    sink.try_send(reading(3.0)).unwrap();
    let current = receiver.try_recv().unwrap();
    let mut forwarded = false;
    assert_eq!(
        current.forward_if_current(|_| {
            forwarded = true;
            Ok(())
        }),
        Ok(true)
    );
    assert!(forwarded);
}
