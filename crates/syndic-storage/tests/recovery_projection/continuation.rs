use super::*;

#[test]
fn pending_continuation_recovers_only_its_completed_prefix_across_reopen() {
    let home = TestHome::new("continuation-pending-prefix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = seed_recovery_fixture_with_kind(
        &store,
        &storage,
        120,
        &[(LIFECYCLE_CONTINUATION_TEXT, TurnLifecycle::Complete)],
        true,
        TurnKind::BerylLifecycleContinuation,
    );
    let projection = prepare_ready(&store, &storage, &fixture, Some(100_000));
    assert_eq!(
        projection.represented_prefix().tail(),
        Some(fixture.represented_tail)
    );
    assert_ne!(
        projection.represented_prefix().tail(),
        projection.selected_path().tail()
    );
    let expected = vec![(
        RecoveryItemSequenceRole::UserInputText,
        LIFECYCLE_CONTINUATION_TEXT.to_owned(),
    )];
    assert_eq!(replay(&storage, &store, projection), expected);
    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let recovered = prepare_ready(&reopened, &storage, &fixture, Some(100_000));
    assert_eq!(recovered.sequence_digest(), projection.sequence_digest());
    assert_eq!(replay(&storage, &reopened, recovered), expected);
    reopened.close().unwrap();
}

#[test]
fn provider_operation_is_not_a_pending_execution_or_replayable_conversation() {
    for pending in [false, true] {
        let home = TestHome::new("provider-operation-recovery-rejection");
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let fixture = seed_recovery_fixture_with_kind(
            &store,
            &storage,
            122,
            &[("provider operation", TurnLifecycle::Complete)],
            pending,
            TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction),
        );
        let revision = storage.revision(&store).unwrap();
        let request = if pending {
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                fixture.thread,
                fixture.selected,
                Some(100_000),
            )
        } else {
            RecoveryProjectionRequest::for_current_selected_path(
                fixture.thread,
                fixture.selected,
                Some(100_000),
            )
        };
        let result = storage.prepare_recovery_projection(&store, request);
        if pending {
            assert!(matches!(
                result,
                Err(RecoveryProjectionError::CurrentTailNotPendingOrdinaryUser)
            ));
        } else {
            assert!(matches!(
                result,
                Err(RecoveryProjectionError::UnsupportedHistory { .. })
            ));
        }
        assert_eq!(storage.revision(&store).unwrap(), revision);
        store.close().unwrap();
    }
}
