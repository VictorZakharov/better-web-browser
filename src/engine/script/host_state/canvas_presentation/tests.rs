use super::*;

fn owner(id: u128) -> NodeId {
    NodeId::from_wire((1_u128 << 64) | id).unwrap()
}

#[test]
fn transfer_moves_the_allocation_once_and_checks_owner_and_extent() {
    let mut transfers = Transfers::default();
    assert!(!transfers.available());
    assert!(transfers.record(owner(1), (1, 1, vec![1; 4])).is_none());
    assert!(transfers.begin());
    assert!(
        !transfers.begin(),
        "nested capture must not discard pending pixels"
    );
    let pixels = vec![9; 24];
    let allocation = pixels.as_ptr();
    let (width, height, token) = transfers.record(owner(1), (3, 2, pixels)).unwrap();
    let mut batch = transfers.finish();
    assert!(!transfers.available());
    assert!(transfers.records.is_empty());
    assert_eq!(transfers.bytes, 0);
    assert!(batch.take(token, owner(2), width, height).is_none());
    assert!(batch.take(token, owner(1), height, width).is_none());
    assert!(batch.take(token + 1, owner(1), width, height).is_none());
    let actual = batch.take(token, owner(1), width, height).unwrap();
    assert_eq!(actual.as_ptr(), allocation);
    assert_eq!(actual, vec![9; 24]);
    assert!(batch.take(token, owner(1), width, height).is_none());
}

#[test]
fn abandoned_batches_retire_pixels_and_tokens_are_not_reused() {
    let mut transfers = Transfers::default();
    assert!(transfers.begin());
    let (_, _, first) = transfers.record(owner(1), (1, 1, vec![7; 4])).unwrap();
    drop(transfers.finish());
    assert!(transfers.begin());
    let (_, _, second) = transfers.record(owner(1), (1, 1, vec![8; 4])).unwrap();
    let mut batch = transfers.finish();
    assert_ne!(first, second);
    assert!(batch.take(first, owner(1), 1, 1).is_none());
    assert_eq!(batch.take(second, owner(1), 1, 1).unwrap(), [8; 4]);
}

#[test]
fn malformed_exhausted_and_over_capacity_transfers_decline_without_retaining() {
    let mut transfers = Transfers::default();
    assert!(transfers.begin());
    for bitmap in [
        (0, 1, vec![]),
        (1, 1, vec![0; 3]),
        (u32::MAX, u32::MAX, vec![]),
    ] {
        assert!(transfers.record(owner(1), bitmap).is_none());
    }
    for _ in 0..MAX_PRESENTED_CANVASES {
        assert!(transfers.record(owner(1), (1, 1, vec![0; 4])).is_some());
    }
    assert!(!transfers.available());
    assert!(transfers.record(owner(1), (1, 1, vec![0; 4])).is_none());
    drop(transfers.finish());
    assert!(transfers.begin());
    transfers.bytes = MAX_BYTES - 3;
    assert!(transfers.record(owner(1), (1, 1, vec![0; 4])).is_none());
    assert!(transfers.records.is_empty());
    transfers.bytes = 0;
    transfers.next = u32::MAX;
    assert!(!transfers.available());
    assert!(transfers.record(owner(1), (1, 1, vec![0; 4])).is_none());
}
