use beryl_model::InputGateRevision;
use syndic_storage::{
    ConsumedRepairRequest, RepairRequestAttemptNonce, RepairRequestDisposition,
    RepairRequestDispositionError,
};

fn revision(value: u64) -> InputGateRevision {
    InputGateRevision::new(value).unwrap()
}

#[test]
fn consumed_request_preserves_exact_nonce_and_transition() {
    let bytes = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 254, 255];
    let nonce = RepairRequestAttemptNonce::from_bytes(bytes);
    let consumed = ConsumedRepairRequest::new(nonce, revision(7), revision(8)).unwrap();
    assert_eq!(consumed.attempt_nonce().as_bytes(), &bytes);
    assert_eq!(consumed.source_gate_revision(), revision(7));
    assert_eq!(consumed.successor_gate_revision(), revision(8));
    assert_ne!(
        RepairRequestDisposition::Available,
        RepairRequestDisposition::Consumed(consumed),
    );
}

#[test]
fn consumed_identity_includes_nonce_and_both_revisions() {
    let nonce = RepairRequestAttemptNonce::from_bytes([4; 16]);
    let original = ConsumedRepairRequest::new(nonce, revision(1), revision(2)).unwrap();
    let another_attempt = ConsumedRepairRequest::new(
        RepairRequestAttemptNonce::from_bytes([5; 16]),
        revision(1),
        revision(2),
    )
    .unwrap();
    let another_transition = ConsumedRepairRequest::new(nonce, revision(2), revision(3)).unwrap();
    assert_ne!(original, another_attempt);
    assert_ne!(original, another_transition);
}

#[test]
fn consumed_request_rejects_equal_reversed_skipped_and_wrapped_revisions() {
    let nonce = RepairRequestAttemptNonce::from_bytes([0; 16]);
    for (source, successor) in [(1, 1), (2, 1), (1, 3), (u64::MAX, 1), (u64::MAX, u64::MAX)] {
        assert_eq!(
            ConsumedRepairRequest::new(nonce, revision(source), revision(successor)),
            Err(RepairRequestDispositionError::InvalidRevisionTransition),
            "unexpected admission for {source} -> {successor}",
        );
    }
}

#[test]
fn revision_boundary_allows_last_transition_and_rejects_zero() {
    assert!(InputGateRevision::new(0).is_err());
    let nonce = RepairRequestAttemptNonce::from_bytes([255; 16]);
    let consumed =
        ConsumedRepairRequest::new(nonce, revision(u64::MAX - 1), revision(u64::MAX)).unwrap();
    assert_eq!(consumed.successor_gate_revision().get(), u64::MAX);
    assert_eq!(consumed.attempt_nonce().as_bytes(), &[255; 16]);
}
