use super::*;
use std::cell::Cell;

#[test]
fn renderer_pcm_handoff_is_bounded_and_retires_document_before_dequeue() {
    let (commands, receiver) = mpsc::sync_channel(MAX_PENDING_GRAPH_CHUNKS);
    let pump = GraphPcmPump {
        commands,
        state: Arc::new(Mutex::new(GraphState {
            document_id: 11,
            context_id: 0,
            last_context_id: 0,
            generation: 1,
        })),
        fault: Arc::new(Mutex::new(None)),
    };
    let format = GraphPcmFormat {
        sample_rate: 48_000,
        channels: 2,
    };
    for _ in 0..MAX_PENDING_GRAPH_CHUNKS {
        assert!(pump.try_queue(11, 13, format, vec![0; 512]).unwrap());
    }
    assert!(!pump.try_queue(11, 13, format, vec![0; 512]).unwrap());
    pump.retire(11);
    assert!(!pump.try_queue(11, 13, format, vec![0; 512]).unwrap());
    for _ in 0..MAX_PENDING_GRAPH_CHUNKS {
        assert!(matches!(
            receiver.try_recv(),
            Ok(GraphCommand::Queue {
                document_id: 11,
                ..
            })
        ));
    }
    pump.activate(12);
    assert!(pump.try_queue(12, 13, format, vec![0; 512]).unwrap());
    assert!(!pump.try_queue(11, 13, format, vec![0; 512]).unwrap());
    assert!(pump.close(12, 13));
    assert!(!pump.try_queue(12, 13, format, vec![0; 512]).unwrap());
    // A close can leave stale commands queued, but cannot admit beyond the fixed bound.
    assert!(!pump.try_queue(12, 14, format, vec![0; 512]).unwrap());
    assert!(!pump.try_queue(12, 15, format, vec![0; 512]).unwrap());
    for _ in 0..MAX_PENDING_GRAPH_CHUNKS {
        receiver.try_recv().unwrap();
    }
    assert!(pump.try_queue(12, 14, format, vec![0; 512]).unwrap());
    assert!(!pump.try_queue(12, 15, format, vec![0; 512]).unwrap());
    assert!(!pump.close(12, 13));
    assert!(pump.close(12, 14));
}

#[test]
fn worker_backpressure_retries_the_same_chunk_until_accepted_or_cancelled() {
    let mut attempts = 0;
    let result = retry_backpressure(
        || {
            attempts += 1;
            Ok(if attempts < 3 {
                GraphPcmStatus::Backpressure
            } else {
                GraphPcmStatus::Accepted
            })
        },
        || true,
        || {},
        GRAPH_PCM_STALL_TIMEOUT,
    );
    assert_eq!(result.unwrap(), Some(GraphPcmStatus::Accepted));
    assert_eq!(attempts, 3);

    let current = Cell::new(true);
    let mut attempts = 0;
    let result = retry_backpressure(
        || {
            attempts += 1;
            Ok(GraphPcmStatus::Backpressure)
        },
        || current.get(),
        || current.set(false),
        GRAPH_PCM_STALL_TIMEOUT,
    );
    assert_eq!(result.unwrap(), None);
    assert_eq!(attempts, 1);

    let result = retry_backpressure(
        || Ok(GraphPcmStatus::Backpressure),
        || true,
        || {},
        Duration::ZERO,
    );
    assert_eq!(
        result.unwrap_err(),
        "graph PCM output remained backpressured"
    );
}

#[test]
fn stale_worker_fault_cannot_poison_the_next_document() {
    let state = Mutex::new(GraphState {
        document_id: 12,
        context_id: 14,
        last_context_id: 14,
        generation: 7,
    });
    let fault = Mutex::new(None);
    record_fault_if_current(&state, &fault, 11, 13, 6, "old failure".into());
    assert!(fault.lock().unwrap().is_none());
    record_fault_if_current(&state, &fault, 12, 14, 7, "current failure".into());
    assert_eq!(fault.lock().unwrap().as_deref(), Some("current failure"));
}
