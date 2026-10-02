use super::*;
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidatePublicationCommandErrorV1,
};

#[test]
fn candidate_source_admission_rejects_stale_foreign_and_substituted_authority() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("candidate-seal-authority", 10, faults.clone());
    let selected = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &selected, 12, 13);
    let expected = DraftEditorCandidateActivationBindingV1::from_head(&session);
    let (_foreign_home, foreign_store, foreign, foreign_thread) =
        fixture("candidate-seal-foreign", 20);
    let foreign_source = current(&foreign, &foreign_store, foreign_thread)
        .draft()
        .piece_root();
    let request = DraftMarkerSealRequestV1::new(
        session.newest_root(),
        DraftMarkerSealOperationIdV1::from_bytes([14; 16]),
    );
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    fresh
        .validate_draft_editor_candidate_candidate(&access, expected)
        .unwrap();
    assert!(
        storage
            .validate_draft_editor_candidate_candidate(&access, expected)
            .is_err()
    );
    assert!(
        foreign
            .validate_draft_editor_candidate_candidate(&access, expected)
            .is_err()
    );
    let stale = DraftEditorCandidateActivationBindingV1::new(
        expected.draft_id(),
        expected.session_id(),
        expected.session_generation() + 1,
        expected.candidate_generation(),
        expected.root(),
        expected.history(),
        expected.logical_extent(),
    );
    assert!(matches!(
        fresh.validate_draft_editor_candidate_candidate(&access, stale),
        Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant)
    ));
    assert!(
        storage
            .prepare_draft_marker_seal_begin_candidate(&access, request)
            .is_err()
    );
    assert!(
        foreign
            .prepare_draft_marker_seal_begin_candidate(&access, request)
            .is_err()
    );
    assert!(
        fresh
            .prepare_draft_marker_seal_begin_candidate(
                &access,
                DraftMarkerSealRequestV1::new(foreign_source, request.operation_id(),)
            )
            .is_err()
    );
    assert!(matches!(
        fresh
            .draft_marker_seal_status_candidate(&access, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Absent
    ));
    assert!(
        fresh
            .prepare_draft_marker_seal_cancel_candidate(&access, request.key())
            .is_err()
    );
    assert_eq!(access.home_revision().unwrap(), revision);
    recovery.publish().unwrap().close().unwrap();
}

#[test]
fn candidate_seal_identity_collision_and_corruption_never_produce_proofs() {
    for corrupt in [false, true] {
        let faults = FaultController::new();
        let (_home, store, storage, thread) =
            fixture_with_faults("candidate-seal-invalid", 30, faults.clone());
        let source = current(&storage, &store, thread).draft().piece_root();
        let request = DraftMarkerSealRequestV1::new(
            source,
            DraftMarkerSealOperationIdV1::from_bytes([32; 16]),
        );
        let begin = storage
            .prepare_draft_marker_seal_begin(&store, request)
            .unwrap();
        committed(execute(
            &store,
            storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
        ));
        let key = if corrupt {
            inject_draft_marker_seal_record_corruption_for_test(
                &store,
                storage.clone(),
                request.key(),
            );
            request.key()
        } else {
            let (key, contribution) = inject_draft_marker_seal_natural_identity_collision_for_test(
                &store,
                storage.clone(),
                request.key(),
                DraftMarkerSealOperationIdV1::from_bytes([33; 16]),
            );
            committed(execute(&store, contribution));
            key
        };
        fail_home(&store, &faults);
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            fresh
                .draft_marker_seal_status_candidate(&access, key)
                .is_err()
        );
        assert!(
            fresh
                .prepare_draft_marker_seal_advance_candidate(&access, key)
                .is_err()
        );
        assert!(
            fresh
                .prepare_draft_marker_seal_cancel_candidate(&access, key)
                .is_err()
        );
        assert!(
            fresh
                .prepare_draft_marker_seal_fail_candidate(
                    &access,
                    key,
                    DraftMarkerSealFailureReasonV1::Operational
                )
                .is_err()
        );
        assert!(
            fresh
                .prepare_draft_marker_seal_supersede_candidate(
                    &access,
                    key,
                    DraftMarkerSealOperationIdV1::from_bytes([34; 16])
                )
                .is_err()
        );
        recovery.abort().close().unwrap();
    }
}

#[test]
fn candidate_read_confirmation_failure_blocks_preparation_and_graph_publication() {
    for admission in [false, true] {
        let faults = FaultController::new();
        let (_home, store, storage, thread) =
            fixture_with_faults("candidate-seal-confirmation", 40, faults.clone());
        let selected = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &selected, 42, 43);
        let request = DraftMarkerSealRequestV1::new(
            session.newest_root(),
            DraftMarkerSealOperationIdV1::from_bytes([44; 16]),
        );
        fail_home(&store, &faults);
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        if admission {
            assert!(
                fresh
                    .validate_draft_editor_candidate_candidate(
                        &access,
                        DraftEditorCandidateActivationBindingV1::from_head(&session)
                    )
                    .is_err()
            );
        } else {
            assert!(
                fresh
                    .prepare_draft_marker_seal_begin_candidate(&access, request)
                    .is_err()
            );
        }
        let failed = recovery.publish().unwrap_err().into_parts().1.abort();
        failed.close().unwrap();
    }
}
