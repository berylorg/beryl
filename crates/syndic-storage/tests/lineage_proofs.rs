use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration,
    RecoveryItemSequenceDigest, SyndicPathDigest, SyndicTurnId, ThreadRevision,
};
use syndic_storage::{
    CasLineageMode, CasLineageProof, CasRepresentedPrefixProof, NativeCasLineage,
    RecoveredInjectionProof, RecoveryItemCount, RecoveryProjectionVersion, RecoveryUtf8ByteCount,
    SyndicTimestamp, SyndicValueError,
};

fn represented_prefix() -> CasRepresentedPrefixProof {
    CasRepresentedPrefixProof::new(
        Some(SyndicTurnId::from_bytes([1; 16])),
        ThreadRevision::new(2).unwrap(),
        SyndicPathDigest::from_bytes([3; 32]),
    )
}

#[test]
fn native_lineage_carries_its_exact_mechanism_and_path() {
    let proof = CasLineageProof::native(NativeCasLineage::Fork, represented_prefix()).unwrap();

    assert_eq!(proof.mode(), CasLineageMode::Native);
    assert_eq!(proof.established_prefix(), represented_prefix());
    assert!(matches!(
        proof,
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Fork,
            ..
        }
    ));
}

#[test]
fn recovered_lineage_requires_exact_session_generations_and_counts() {
    let recovered = RecoveredInjectionProof::new(
        RecoveryProjectionVersion::V1,
        represented_prefix(),
        RecoveryItemSequenceDigest::from_bytes([4; 32]),
        RecoveryItemCount::new(6).unwrap(),
        RecoveryUtf8ByteCount::new(100).unwrap(),
        SyndicTimestamp::from_unix_millis(9),
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(7).unwrap(),
            CasLoadedThreadGeneration::new(8).unwrap(),
        ),
    )
    .unwrap();
    let proof = CasLineageProof::recovered(recovered);

    assert_eq!(proof.mode(), CasLineageMode::RecoveredInjection);
    assert_eq!(proof.established_prefix(), represented_prefix());
    assert_eq!(recovered.item_count().get(), 6);
    assert_eq!(recovered.completed_at().unix_millis(), 9);
    assert_eq!(recovered.loaded_generation().process().get(), 7);
    assert_eq!(recovered.loaded_generation().thread().get(), 8);
}

#[test]
fn recovery_counts_enforce_nonempty_and_exact_byte_ceiling() {
    assert!(matches!(
        RecoveryItemCount::new(0),
        Err(SyndicValueError::ZeroCount { .. })
    ));
    assert!(RecoveryUtf8ByteCount::new(RecoveryUtf8ByteCount::MAX).is_ok());
    assert!(RecoveryItemCount::new(RecoveryItemCount::MAX).is_ok());
    assert!(matches!(
        RecoveryItemCount::new(RecoveryItemCount::MAX + 1),
        Err(SyndicValueError::CountTooLarge {
            kind: "recovery item count",
            maximum: RecoveryItemCount::MAX,
            actual,
        }) if actual == RecoveryItemCount::MAX + 1
    ));
    assert!(matches!(
        RecoveryUtf8ByteCount::new(RecoveryUtf8ByteCount::MAX + 1),
        Err(SyndicValueError::CountTooLarge { .. })
    ));
}

#[test]
fn empty_selected_path_is_explicit_not_missing_proof() {
    let empty = CasRepresentedPrefixProof::new(
        None,
        ThreadRevision::new(1).unwrap(),
        SyndicPathDigest::from_bytes([0; 32]),
    );
    let proof = CasLineageProof::native(NativeCasLineage::Fresh, empty).unwrap();

    assert_eq!(proof.established_prefix().tail(), None);
}

#[test]
fn lineage_mechanisms_reject_incompatible_path_shapes() {
    let empty = CasRepresentedPrefixProof::new(
        None,
        ThreadRevision::new(1).unwrap(),
        SyndicPathDigest::from_bytes([0; 32]),
    );

    for mechanism in [
        NativeCasLineage::Continuation,
        NativeCasLineage::Resume,
        NativeCasLineage::Fork,
    ] {
        assert!(matches!(
            CasLineageProof::native(mechanism, empty),
            Err(SyndicValueError::InvalidLineageProof { .. })
        ));
    }
    assert!(matches!(
        CasLineageProof::native(NativeCasLineage::Fresh, represented_prefix()),
        Err(SyndicValueError::InvalidLineageProof { .. })
    ));
    assert!(matches!(
        RecoveredInjectionProof::new(
            RecoveryProjectionVersion::V1,
            empty,
            RecoveryItemSequenceDigest::from_bytes([4; 32]),
            RecoveryItemCount::new(1).unwrap(),
            RecoveryUtf8ByteCount::new(1).unwrap(),
            SyndicTimestamp::from_unix_millis(1),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
        ),
        Err(SyndicValueError::InvalidLineageProof { .. })
    ));
    assert!(matches!(
        RecoveredInjectionProof::new(
            RecoveryProjectionVersion::V1,
            represented_prefix(),
            RecoveryItemSequenceDigest::from_bytes([5; 32]),
            RecoveryItemCount::new(2).unwrap(),
            RecoveryUtf8ByteCount::new(1).unwrap(),
            SyndicTimestamp::from_unix_millis(2),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(2).unwrap(),
                CasLoadedThreadGeneration::new(2).unwrap(),
            ),
        ),
        Err(SyndicValueError::InvalidLineageProof {
            reason: "recovered nonempty item count exceeds its UTF-8 byte count"
        })
    ));
}
