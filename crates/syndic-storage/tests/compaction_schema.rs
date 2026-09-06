use beryl_model::{BerylHomeId, SyndicThreadId};
use syndic_storage::{
    CompactionOperationId, CompactionOperationNonce, ContentEncoding, LIFECYCLE_CONTINUATION_TEXT,
    derive_lifecycle_continuation_item_id, derive_lifecycle_continuation_turn_id,
    prepare_lifecycle_continuation_content,
};

fn operation() -> CompactionOperationId {
    CompactionOperationId::new(
        SyndicThreadId::from_bytes([3; 16]),
        CompactionOperationNonce::from_bytes([7; 16]),
    )
}

#[test]
fn provider_turn_identity_is_the_exact_operation_nonce_payload() {
    let operation = operation();

    assert_eq!(
        operation.provider_turn_id().as_bytes(),
        operation.nonce().as_bytes()
    );
}

#[test]
fn lifecycle_content_is_exactly_one_fixed_text_atom() {
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let mut expected = vec![1];
    expected.extend_from_slice(&1_u64.to_be_bytes());
    expected.push(0);
    expected.extend_from_slice(&(LIFECYCLE_CONTINUATION_TEXT.len() as u64).to_be_bytes());
    expected.extend_from_slice(LIFECYCLE_CONTINUATION_TEXT.as_bytes());

    assert_eq!(prepared.encoding(), ContentEncoding::ComposerV1);
    assert_eq!(prepared.summary().atom_count(), 1);
    assert_eq!(prepared.summary().image_marker_count(), 0);
    assert_eq!(
        prepared.summary().logical_utf8_bytes(),
        LIFECYCLE_CONTINUATION_TEXT.len() as u64
    );
    assert_eq!(prepared.chunks().len(), 1);
    assert_eq!(prepared.chunks()[0].bytes(), expected);
}

#[test]
fn lifecycle_turn_and_item_domains_are_stable_and_distinct() {
    let home = BerylHomeId::from_bytes([11; 16]);
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let digest = prepared.summary().digest();

    let turn = derive_lifecycle_continuation_turn_id(home, operation(), digest);
    let item = derive_lifecycle_continuation_item_id(home, operation(), digest);

    assert_ne!(turn.as_bytes(), item.as_bytes());
    assert_eq!(
        turn,
        derive_lifecycle_continuation_turn_id(home, operation(), digest)
    );
    assert_eq!(
        item,
        derive_lifecycle_continuation_item_id(home, operation(), digest)
    );
}
