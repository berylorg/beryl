#![cfg(feature = "test-faults")]

include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use syndic_storage::{
    DRAFT_MARKER_ADMISSION_MAX_HEADS, DraftMarkerAdmissionCommandIdV1,
    DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerLabelReadinessDispositionV1, DraftMarkerLabelReadinessPageAttemptV1,
    DraftMarkerLabelReadinessPageRequestV1, DraftMarkerLabelReadinessPageSubmissionFlightV1,
    DraftMarkerLabelReadinessPageSubmissionOutcomeV1,
    DraftMarkerLabelReadinessPageSubmissionRefusalV1, DraftMarkerReadinessCandidateSourceV1,
    DraftMarkerReadinessSourceAssociationV1, DraftMarkerReadinessSourceSelectorV1,
};

#[test]
fn prepared_and_ready_flight_drops_release_the_bounded_runtime_slot() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) = fixture_with_faults("phase222-attempt-bound", 90, faults);
    let (session, marker) = marked_session(&storage, &store, thread, 91);
    let mut held = Vec::new();
    for seed in 0..DRAFT_MARKER_ADMISSION_MAX_HEADS - 1 {
        held.push(attempt(
            &storage,
            &store,
            admission_owner(&session, seed as u8),
            seed as u8,
            1,
            false,
            seed.wrapping_add(100) as u8,
            &session,
            marker.marker_id(),
        ));
    }
    let mut ready = attempt(
        &storage,
        &store,
        admission_owner(&session, 70),
        71,
        1,
        false,
        72,
        &session,
        marker.marker_id(),
    );
    let receipt = store.compose_proof(ready.take_command().unwrap()).unwrap();
    let ready = ready.into_submission_flight(&store, receipt).unwrap();

    let overflow = || {
        DraftMarkerLabelReadinessPageRequestV1::new(
            admission_owner(&session, 73),
            DraftMarkerAdmissionCommandIdV1::from_bytes([74; 16]),
            NonZeroU64::new(1).unwrap(),
            false,
            DraftMarkerLabelReadinessDispositionV1::Reuse,
            Box::new([association(75, &session, marker.marker_id())]),
            None,
        )
    };
    assert!(matches!(
        storage.prepare_draft_marker_label_readiness_page(&store, overflow()),
        Err(syndic_storage::DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    drop(ready);
    assert!(
        storage
            .prepare_draft_marker_label_readiness_page(&store, overflow())
            .is_ok()
    );
    drop(held);
}

#[test]
fn exact_pairing_one_shot_and_final_eof_deferral() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) = fixture_with_faults("phase222-linear", 1, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 2);
    let owner = admission_owner(&session, 10);

    let mut first = attempt(
        &storage,
        &store,
        owner,
        20,
        1,
        false,
        30,
        &session,
        marker.marker_id(),
    );
    let mut substitute = attempt(
        &storage,
        &store,
        admission_owner(&session, 11),
        21,
        1,
        false,
        31,
        &session,
        marker.marker_id(),
    );
    assert!(first.take_command().is_some());
    assert!(first.take_command().is_none());
    let substitute_receipt = store
        .compose_proof(substitute.take_command().unwrap())
        .unwrap();
    drop(substitute);
    assert!(
        first
            .into_submission_flight(&store, substitute_receipt)
            .is_err()
    );

    let flight = make_flight(
        &storage,
        &store,
        owner,
        22,
        1,
        false,
        32,
        &session,
        marker.marker_id(),
    );
    assert_advanced(
        "initial direct commit",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        false,
    );
    let obsolete = make_flight(
        &storage,
        &store,
        owner,
        22,
        1,
        false,
        32,
        &session,
        marker.marker_id(),
    );
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, obsolete),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Obsolete
        )
    ));

    let eof_owner = admission_owner(&session, 40);
    let eof = make_flight(
        &storage,
        &store,
        eof_owner,
        41,
        1,
        true,
        42,
        &session,
        marker.marker_id(),
    );
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, eof),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::FinalEvidenceEof
        )
    ));
    assert!(snapshot(&storage, &store, eof_owner).head().is_none());
}

#[test]
fn after_persist_finalizes_local_flight_without_publishing_success() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase222-after-persist", 50, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 51);
    let owner = admission_owner(&session, 52);
    let flight = make_flight(
        &storage,
        &store,
        owner,
        53,
        1,
        false,
        54,
        &session,
        marker.marker_id(),
    );
    faults.fail_next(FaultPoint::AfterPersist);
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable
        )
    ));
}

#[test]
fn ready_flight_is_rejected_after_its_home_generation_retires() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase222-ready-generation", 55, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 56);
    let owner = admission_owner(&session, 57);
    let flight = make_flight(
        &storage,
        &store,
        owner,
        58,
        1,
        false,
        59,
        &session,
        marker.marker_id(),
    );

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable
        )
    ));
    assert!(snapshot(&storage, &store, owner).head().is_none());
}

#[test]
fn indeterminate_exact_new_reopens_same_generation_and_finishes_the_page() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase222-indeterminate", 60, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 61);
    let owner = admission_owner(&session, 62);
    let associations = Box::new([
        association(70, &session, marker.marker_id()),
        association(71, &session, marker.marker_id()),
        association(72, &session, marker.marker_id()),
    ]);

    let pending = page_flight(&storage, &store, owner, 63, 1, false, associations.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let pending = match storage.submit_draft_marker_label_readiness_page(&store, pending) {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("indeterminate submission did not retain an opaque reconciliation flight"),
    };
    assert!(matches!(
        storage.prepare_draft_marker_label_readiness_page(
            &store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([64; 16]),
                NonZeroU64::new(1).unwrap(),
                false,
                DraftMarkerLabelReadinessDispositionV1::Reuse,
                associations.clone(),
                None,
            ),
        ),
        Err(syndic_storage::DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    assert_advanced(
        "same-generation exact-new",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, pending),
        false,
    );
    assert_progress(&storage, &store, owner, 1, 1);

    let second = page_flight(&storage, &store, owner, 63, 1, false, associations.clone());
    assert_advanced(
        "second page quantum",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, second),
        false,
    );
    assert_progress(&storage, &store, owner, 2, 2);

    let final_quantum = page_flight(&storage, &store, owner, 63, 1, false, associations.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let final_quantum = match storage
        .submit_draft_marker_label_readiness_page(&store, final_quantum)
    {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("indeterminate final quantum did not retain reconciliation custody"),
    };
    assert_advanced(
        "final reconciled quantum",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, final_quantum),
        false,
    );
    assert_progress(&storage, &store, owner, 3, 0);

    let replay = page_flight(&storage, &store, owner, 63, 1, false, associations);
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, replay),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Obsolete
        )
    ));
}

#[test]
fn reconciliation_error_preserves_exact_pending_owner_for_retry() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase222-reconciliation-retry", 120, faults.clone());
    let (_foreign_home, foreign_store, _foreign_storage, _foreign_thread) =
        fixture_with_faults("phase222-reconciliation-retry-foreign", 121, FaultController::new());
    let (session, marker) = marked_session(&storage, &store, thread, 122);
    let owner = admission_owner(&session, 123);
    let associations = Box::new([
        association(124, &session, marker.marker_id()),
        association(125, &session, marker.marker_id()),
    ]);
    let flight = page_flight(&storage, &store, owner, 126, 1, false, associations.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let pending = match storage.submit_draft_marker_label_readiness_page(&store, flight) {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("indeterminate submission did not retain reconciliation custody"),
    };

    let pending = match storage
        .submit_draft_marker_label_readiness_page(&foreign_store, pending)
    {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("reconciliation error did not preserve the exact pending flight"),
    };
    assert!(matches!(
        storage.prepare_draft_marker_label_readiness_page(
            &store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([127; 16]),
                NonZeroU64::new(1).unwrap(),
                false,
                DraftMarkerLabelReadinessDispositionV1::Reuse,
                associations,
                None,
            ),
        ),
        Err(syndic_storage::DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    assert_advanced(
        "exact retry after reconciliation error",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, pending),
        false,
    );
    assert_progress(&storage, &store, owner, 1, 1);
}

#[test]
fn lost_ack_reconciles_after_recovery_without_reviving_the_prior_generation() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase222-recovered-ack", 110, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 111);
    let owner = admission_owner(&session, 112);
    let associations = Box::new([
        association(113, &session, marker.marker_id()),
        association(114, &session, marker.marker_id()),
    ]);
    let flight = page_flight(&storage, &store, owner, 115, 1, false, associations.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let pending = match storage.submit_draft_marker_label_readiness_page(&store, flight) {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("lost acknowledgement did not retain reconciliation custody"),
    };

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    assert_advanced(
        "recovered exact-new",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, pending),
        false,
    );
    assert_progress(&storage, &store, owner, 1, 1);
    assert!(matches!(
        storage.prepare_draft_marker_label_readiness_page(
            &store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([115; 16]),
                NonZeroU64::new(1).unwrap(),
                false,
                DraftMarkerLabelReadinessDispositionV1::Reuse,
                associations,
                None,
            ),
        ),
        Err(syndic_storage::DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
}

#[test]
fn collision_is_operation_scoped_and_refusal_releases_runtime_capacity() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase222-collision", 80, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 81);
    let owner = admission_owner(&session, 82);
    let first = make_flight(
        &storage,
        &store,
        owner,
        83,
        1,
        false,
        84,
        &session,
        marker.marker_id(),
    );
    assert_advanced(
        "collision fixture initial commit",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, first),
        false,
    );

    let collision = make_flight(
        &storage,
        &store,
        owner,
        85,
        2,
        false,
        84,
        &session,
        marker.marker_id(),
    );
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, collision),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Collision
    ));
    let unrelated = make_flight(
        &storage,
        &store,
        owner,
        86,
        2,
        false,
        87,
        &session,
        marker.marker_id(),
    );
    assert_advanced(
        "post-collision unrelated quantum",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, unrelated),
        false,
    );

    for seed in 0_u8..65 {
        let transient_owner = admission_owner(&session, seed.wrapping_add(100));
        let transient = make_flight(
            &storage,
            &store,
            transient_owner,
            seed.wrapping_add(120),
            1,
            true,
            seed.wrapping_add(140),
            &session,
            marker.marker_id(),
        );
        let outcome = storage.submit_draft_marker_label_readiness_page(&store, transient);
        assert!(
            matches!(
                outcome,
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                    DraftMarkerLabelReadinessPageSubmissionRefusalV1::FinalEvidenceEof
                )
            ),
            "transient seed {seed} did not reach the determinate EOF refusal"
        );
    }
}

fn attempt(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: u64,
    eof: bool,
    target: u8,
    session: &DraftEditorCandidateSessionV1,
    marker: SyndicDraftMarkerId,
) -> DraftMarkerLabelReadinessPageAttemptV1 {
    storage
        .prepare_draft_marker_label_readiness_page(
            store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
                NonZeroU64::new(ordinal).unwrap(),
                eof,
                DraftMarkerLabelReadinessDispositionV1::Reuse,
                Box::new([association(target, session, marker)]),
                None,
            ),
        )
        .unwrap()
}

fn make_flight(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: u64,
    eof: bool,
    target: u8,
    session: &DraftEditorCandidateSessionV1,
    marker: SyndicDraftMarkerId,
) -> DraftMarkerLabelReadinessPageSubmissionFlightV1 {
    let mut attempt = attempt(
        storage, store, owner, command, ordinal, eof, target, session, marker,
    );
    let receipt = store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    attempt.into_submission_flight(store, receipt).unwrap()
}

fn page_flight(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: u64,
    eof: bool,
    associations: Box<[DraftMarkerReadinessSourceAssociationV1]>,
) -> DraftMarkerLabelReadinessPageSubmissionFlightV1 {
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page(
            store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
                NonZeroU64::new(ordinal).unwrap(),
                eof,
                DraftMarkerLabelReadinessDispositionV1::Reuse,
                associations,
                None,
            ),
        )
        .unwrap();
    let receipt = store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    attempt.into_submission_flight(store, receipt).unwrap()
}

fn association(
    target: u8,
    session: &DraftEditorCandidateSessionV1,
    marker: SyndicDraftMarkerId,
) -> DraftMarkerReadinessSourceAssociationV1 {
    DraftMarkerReadinessSourceAssociationV1::new(
        SyndicDraftMarkerId::from_bytes([target; 16]),
        DraftMarkerReadinessSourceSelectorV1::Candidate(
            DraftMarkerReadinessCandidateSourceV1::new(
                session.draft_id(),
                session.session_id(),
                session.newest_candidate_generation(),
                session.newest_root(),
                marker,
            ),
        ),
    )
}

fn admission_owner(
    session: &DraftEditorCandidateSessionV1,
    seed: u8,
) -> DraftMarkerAdmissionOwnerV1 {
    DraftMarkerAdmissionOwnerV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes([seed; 16]),
    )
}

fn marked_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed: u8,
) -> (DraftEditorCandidateSessionV1, DraftPieceMarkerV1) {
    let durable = current(storage, store, thread);
    let mut session = open_session(storage, store, &durable, seed, seed.wrapping_add(1));
    session = complete_staged(
        storage,
        store,
        &session,
        seed.wrapping_add(2),
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let marker = marker(seed.wrapping_add(3), 1, 7);
    session = complete_marker_edit(
        storage,
        store,
        &session,
        seed.wrapping_add(4),
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
    );
    (session, marker)
}

fn complete_marker_edit(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacement: DraftPieceReplacementV1,
) -> DraftEditorCandidateSessionV1 {
    let (prepared, identity, _) = stage_replacement(
        storage,
        store,
        session,
        operation,
        replacement,
        session.logical_extent(),
    );
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    active_session(storage, store, session.draft_id(), session.session_id())
}

fn snapshot(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
) -> syndic_storage::DraftMarkerAdmissionPublicationSnapshotV1 {
    storage
        .draft_marker_admission_publication_snapshot_for_test(store, owner, &[])
        .unwrap()
}

fn assert_progress(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    count: u64,
    cursor: u64,
) {
    let snapshot = snapshot(storage, store, owner);
    let head = snapshot.head().unwrap();
    assert_eq!(head.target_root().count(), count);
    assert_eq!(head.ingestion_association_cursor(), cursor);
}

fn assert_advanced(
    stage: &str,
    _storage: &SyndicStorage,
    store: &HomeStore,
    outcome: DraftMarkerLabelReadinessPageSubmissionOutcomeV1,
    later_failure: bool,
) {
    match outcome {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced {
            receipt,
            later_failure: actual,
        } => {
            assert_eq!(receipt.home_revision(), store.home_revision().unwrap());
            assert_eq!(actual.is_some(), later_failure);
        }
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Retryable => {
            panic!("{stage}: retryable")
        }
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(refusal) => match refusal {
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Obsolete => {
                panic!("{stage}: obsolete")
            }
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::FinalEvidenceEof => {
                panic!("{stage}: final evidence EOF")
            }
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable => {
                panic!("{stage}: unavailable")
            }
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Rejected => {
                panic!("{stage}: rejected")
            }
        },
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Collision => {
            panic!("{stage}: collision")
        }
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(_) => {
            panic!("{stage}: reconciliation pending")
        }
    }
}
