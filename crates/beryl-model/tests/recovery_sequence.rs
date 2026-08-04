use beryl_model::{
    RecoveryItemSequenceAccumulator, RecoveryItemSequenceDigest, RecoveryItemSequenceError,
    RecoveryItemSequenceRole,
};

#[test]
fn incremental_recovery_digest_matches_the_fixed_v1_vector() {
    let mut digest = RecoveryItemSequenceAccumulator::new(2, 2);
    digest
        .begin_item(1, RecoveryItemSequenceRole::UserInputText, 1)
        .unwrap();
    digest.update_text(b"u").unwrap();
    digest.finish_item().unwrap();
    digest
        .begin_item(2, RecoveryItemSequenceRole::AssistantOutputText, 1)
        .unwrap();
    digest.update_text(b"a").unwrap();
    digest.finish_item().unwrap();

    assert_eq!(
        digest.finish().unwrap(),
        RecoveryItemSequenceDigest::from_bytes([
            0x9a, 0x25, 0xb0, 0xeb, 0xf7, 0x06, 0x26, 0xc5, 0xf0, 0x41, 0x84, 0x6d, 0xd5, 0x87,
            0x82, 0xf4, 0x92, 0x43, 0x9e, 0xf7, 0xa9, 0xe8, 0x9b, 0x7d, 0x6e, 0x14, 0x90, 0x53,
            0xa6, 0xc1, 0x79, 0xb2,
        ])
    );
}

#[test]
fn recovery_digest_rejects_structural_disagreement() {
    let mut digest = RecoveryItemSequenceAccumulator::new(1, 2);
    assert_eq!(
        digest
            .begin_item(2, RecoveryItemSequenceRole::UserInputText, 2)
            .unwrap_err(),
        RecoveryItemSequenceError::UnexpectedOrdinal {
            expected: 1,
            actual: 2,
        }
    );
    digest
        .begin_item(1, RecoveryItemSequenceRole::UserInputText, 2)
        .unwrap();
    digest.update_text(b"a").unwrap();
    assert_eq!(
        digest.finish_item().unwrap_err(),
        RecoveryItemSequenceError::ItemStillActive { remaining_bytes: 1 }
    );
}
