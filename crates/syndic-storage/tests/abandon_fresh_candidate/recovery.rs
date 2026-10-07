use crate::{shared::*, support::*};

use beryl_home_store::{CommandCancellation, HomeCandidateRecoveryAccess};
use syndic_storage::{
    DraftEditorCandidatePublicationCommandErrorV1,
    DraftEditorCandidateSessionAbandonFreshOutcomeV1, DraftEditorCandidateSessionRecordKeyV1,
    PreparedDraftEditorCandidateSessionOpenV1,
    test_faults::{DraftCandidatePublicationFault, inject_draft_candidate_publication_fault},
};

fn fail_home(store: &HomeStore, faults: &FaultController) {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
}

fn candidate_execute(
    access: &HomeCandidateRecoveryAccess<'_>,
    contribution: MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(contribution).unwrap();
    access.execute(command)
}

fn prepare_open(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    session: u8,
    operation: u8,
) -> PreparedDraftEditorCandidateSessionOpenV1 {
    storage
        .prepare_open_draft_editor_candidate_session(
            store,
            open_request(&current(storage, store, thread), session, operation),
        )
        .unwrap()
}

#[test]
fn candidate_qualifies_absent_original_opening_without_writes() {
    let disposal_operation = DraftPieceOperationIdV1::from_bytes([14; 16]);
    let (_home, store, storage, faults, thread) =
        fault_fixture("fresh-recovery-absent", 10, 65_536);
    let original = prepare_open(&storage, &store, thread, 12, 13);
    let home_id = store.home_id();
    let revision = storage.revision(&store).unwrap();
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    for _ in 0..2 {
        assert!(matches!(
            fresh
                .qualify_fresh_draft_editor_candidate_session_open_candidate(
                    &access,
                    home_id,
                    disposal_operation,
                    &original,
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Absent
        ));
    }
    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
    recovery.publish().unwrap().close().unwrap();
}

#[test]
fn candidate_recovers_opening_committed_before_classification_and_disposes_exactly_once() {
    let disposal_operation = DraftPieceOperationIdV1::from_bytes([25; 16]);
    let (_home, store, storage, faults, thread) = fault_fixture("fresh-recovery-open", 20, 65_536);
    let original = prepare_open(&storage, &store, thread, 22, 23);
    let substituted = prepare_open(&storage, &store, thread, 22, 24);
    let home_id = store.home_id();
    let selector_before = selector(&current(&storage, &store, thread));
    let outcome = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            original.clone(),
        ),
    );
    assert!(matches!(outcome, CommandOutcome::Committed { .. }));
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    assert!(
        storage
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                home_id,
                disposal_operation,
                &original
            )
            .is_err()
    );
    let (_foreign_home, foreign_store, foreign, _) = fixture("fresh-recovery-foreign", 30, 65_536);
    assert!(
        foreign
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                home_id,
                disposal_operation,
                &original
            )
            .is_err()
    );
    assert!(
        fresh
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                foreign_store.home_id(),
                disposal_operation,
                &original
            )
            .is_err()
    );
    assert!(
        fresh
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                home_id,
                disposal_operation,
                &substituted
            )
            .is_err()
    );
    let opened = match fresh
        .reconcile_draft_editor_candidate_session_open_candidate(&access, &original, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
        other => panic!("original opening did not authenticate: {other:?}"),
    };
    for _ in 0..2 {
        assert_eq!(
            fresh
                .qualify_fresh_draft_editor_candidate_session_open_candidate(
                    &access,
                    home_id,
                    disposal_operation,
                    &original
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Active(opened.clone())
        );
    }
    let request = abandon_request(&opened, 25);
    let prepared = fresh
        .prepare_abandon_fresh_draft_editor_candidate_session_candidate(&access, request)
        .unwrap();
    let outcome = candidate_execute(
        &access,
        fresh.abandon_fresh_draft_editor_candidate_session(
            fresh.revision_candidate(&access).unwrap(),
            prepared.clone(),
        ),
    );
    let disposed = match fresh
        .reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
            &access, &prepared, outcome,
        )
        .unwrap()
    {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(head) => head,
        other => panic!("candidate cleanup did not commit: {other:?}"),
    };
    assert_eq!(
        disposed.disposal_operation_id(),
        Some(request.operation_id())
    );
    for _ in 0..2 {
        assert_eq!(
            fresh
                .qualify_fresh_draft_editor_candidate_session_open_candidate(
                    &access,
                    home_id,
                    disposal_operation,
                    &original
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Disposed(disposed.clone())
        );
    }
    let replay = fresh
        .prepare_abandon_fresh_draft_editor_candidate_session_candidate(&access, request)
        .unwrap();
    let outcome = candidate_execute(
        &access,
        fresh.abandon_fresh_draft_editor_candidate_session(
            fresh.revision_candidate(&access).unwrap(),
            replay.clone(),
        ),
    );
    assert!(matches!(
        fresh
            .reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
                &access, &replay, outcome
            )
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_)
    ));
    let wrong = abandon_request(&disposed, 25);
    let wrong = fresh
        .prepare_abandon_fresh_draft_editor_candidate_session_candidate(&access, wrong)
        .unwrap();
    let outcome = candidate_execute(
        &access,
        fresh.abandon_fresh_draft_editor_candidate_session(
            fresh.revision_candidate(&access).unwrap(),
            wrong.clone(),
        ),
    );
    assert!(matches!(
        fresh
            .reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
                &access, &wrong, outcome
            )
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::OccupiedIdentityCollision(_)
    ));
    let store = recovery.publish().unwrap();
    assert_eq!(selector(&current(&fresh, &store, thread)), selector_before);
    assert_eq!(head(&fresh, &store, &opened), disposed);
    store.close().unwrap();
}

#[test]
fn candidate_original_opening_ambiguity_uses_exact_retained_custody() {
    let disposal_operation = DraftPieceOperationIdV1::from_bytes([44; 16]);
    for (cut, committed_at_cut) in [
        (FaultPoint::BeforeCommit, false),
        (FaultPoint::AfterCommitBeforePersist, true),
        (FaultPoint::AfterPersist, true),
        (FaultPoint::BeforeVerification, true),
    ] {
        let (_home, store, storage, faults, thread) =
            fault_fixture("fresh-recovery-opening-cuts", 40, 65_536);
        let original = prepare_open(&storage, &store, thread, 42, 43);
        let home_id = store.home_id();
        let contribution = storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            original.clone(),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command.add(contribution).unwrap();
        faults.fail_next(cut);
        let outcome = store.execute(command);
        if store.health().state() != HomeHealthState::Failed {
            fail_home(&store, &faults);
        }
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        if committed_at_cut {
            assert!(matches!(
                fresh
                    .reconcile_draft_editor_candidate_session_open_candidate(
                        &access, &original, outcome
                    )
                    .unwrap(),
                DraftEditorCandidateSessionOpenOutcomeV1::Opened(_)
            ));
        } else {
            assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
        }
        assert!(access.pending_reconciliations().is_empty());
        let qualified = fresh
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                home_id,
                disposal_operation,
                &original,
            )
            .unwrap();
        assert!(matches!(
            (committed_at_cut, qualified),
            (false, DraftEditorCandidateSessionReadOutcomeV1::Absent)
                | (true, DraftEditorCandidateSessionReadOutcomeV1::Active(_))
        ));
        recovery.publish().unwrap().close().unwrap();
    }
}

#[test]
fn candidate_disposal_reconciles_each_atomic_cut_and_retries_only_exact_noncommit() {
    let disposal_operation = DraftPieceOperationIdV1::from_bytes([54; 16]);
    for (cut, committed_at_cut) in [
        (FaultPoint::BeforeCommit, false),
        (FaultPoint::AfterCommitBeforePersist, true),
        (FaultPoint::AfterPersist, true),
        (FaultPoint::BeforeVerification, true),
    ] {
        let (_home, store, storage, faults, thread) =
            fault_fixture("fresh-recovery-disposal-cuts", 50, 65_536);
        let original = prepare_open(&storage, &store, thread, 52, 53);
        let home_id = store.home_id();
        committed(execute(
            &store,
            storage.open_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                original.clone(),
            ),
        ));
        fail_home(&store, &faults);
        let mut recovery = store.recover_same_home().unwrap();
        let first = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let opened = match first
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                home_id,
                disposal_operation,
                &original,
            )
            .unwrap()
        {
            DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
            other => panic!("fresh opening not active: {other:?}"),
        };
        let request = abandon_request(&opened, 54);
        let prepared = first
            .prepare_abandon_fresh_draft_editor_candidate_session_candidate(&access, request)
            .unwrap();
        let contribution = first.abandon_fresh_draft_editor_candidate_session(
            first.revision_candidate(&access).unwrap(),
            prepared.clone(),
        );
        let mut command = HomeCommand::new(access.home_revision().unwrap());
        command.add(contribution).unwrap();
        faults.fail_next(cut);
        let outcome = access.execute(command);
        let mut recovery = recovery.abort().recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let result = fresh.reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
            &access, &prepared, outcome,
        );
        if committed_at_cut {
            assert!(matches!(
                result.unwrap(),
                DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
            ));
        } else {
            assert!(matches!(
                result,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
            assert_eq!(
                fresh
                    .qualify_fresh_draft_editor_candidate_session_open_candidate(
                        &access,
                        home_id,
                        disposal_operation,
                        &original
                    )
                    .unwrap(),
                DraftEditorCandidateSessionReadOutcomeV1::Active(opened)
            );
            let retry = candidate_execute(
                &access,
                fresh.abandon_fresh_draft_editor_candidate_session(
                    fresh.revision_candidate(&access).unwrap(),
                    prepared.clone(),
                ),
            );
            assert!(matches!(
                fresh
                    .reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
                        &access, &prepared, retry
                    )
                    .unwrap(),
                DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
            ));
        }
        assert!(access.pending_reconciliations().is_empty());
        assert!(matches!(
            fresh
                .qualify_fresh_draft_editor_candidate_session_open_candidate(
                    &access,
                    home_id,
                    disposal_operation,
                    &original
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
        ));
        assert!(
            first
                .qualify_fresh_draft_editor_candidate_session_open_candidate(
                    &access,
                    home_id,
                    disposal_operation,
                    &original
                )
                .is_err()
        );
        recovery.publish().unwrap().close().unwrap();
    }
}

#[test]
fn candidate_cancelled_disposal_retains_the_exact_active_opening() {
    let disposal_operation = DraftPieceOperationIdV1::from_bytes([64; 16]);
    let (_home, store, storage, faults, thread) =
        fault_fixture("fresh-recovery-cancel", 60, 65_536);
    let original = prepare_open(&storage, &store, thread, 62, 63);
    let home_id = store.home_id();
    committed(execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            original.clone(),
        ),
    ));
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let opened = match fresh
        .qualify_fresh_draft_editor_candidate_session_open_candidate(
            &access,
            home_id,
            disposal_operation,
            &original,
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        other => panic!("opening not active: {other:?}"),
    };
    let prepared = fresh
        .prepare_abandon_fresh_draft_editor_candidate_session_candidate(
            &access,
            abandon_request(&opened, 64),
        )
        .unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let mut command =
        HomeCommand::new(access.home_revision().unwrap()).with_cancellation(cancellation);
    command
        .add(fresh.abandon_fresh_draft_editor_candidate_session(
            fresh.revision_candidate(&access).unwrap(),
            prepared.clone(),
        ))
        .unwrap();
    let outcome = access.execute(command);
    assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
    assert!(matches!(
        fresh.reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
            &access, &prepared, outcome
        ),
        Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
    ));
    assert_eq!(
        fresh
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                home_id,
                disposal_operation,
                &original
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(opened)
    );
    recovery.publish().unwrap().close().unwrap();
}

#[test]
fn candidate_opening_qualification_rejects_dirty_occupied_and_partial_closures() {
    let disposal_operation = DraftPieceOperationIdV1::from_bytes([75; 16]);
    for invalid in 0..6 {
        let (_home, store, storage, faults, thread) =
            fault_fixture("fresh-recovery-invalid", 70, 65_536);
        let original = prepare_open(&storage, &store, thread, 72, 73);
        let home_id = store.home_id();
        let outcome = execute(
            &store,
            storage.open_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                original.clone(),
            ),
        );
        let opened = match storage
            .reconcile_draft_editor_candidate_session_open(&store, &original, outcome)
            .unwrap()
        {
            DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
            other => panic!("opening unavailable: {other:?}"),
        };
        match invalid {
            0 | 1 => {
                let edit = transaction(&storage, &store, &opened, 74, "changed", point(1));
                if invalid == 0 {
                    build(&storage, &store, &edit);
                    committed(execute(
                        &store,
                        storage.settle_draft_piece_edit(
                            storage.revision(&store).unwrap(),
                            edit.prepared,
                        ),
                    ));
                } else {
                    committed(execute(
                        &store,
                        storage.begin_draft_piece_edit(
                            storage.revision(&store).unwrap(),
                            edit.prepared,
                        ),
                    ));
                }
            }
            2 | 3 => {
                let key = if invalid == 2 {
                    DraftEditorCandidateSessionRecordKeyV1::open_receipt(
                        opened.draft_id(),
                        opened.session_id(),
                        opened.open_operation_id(),
                    )
                } else {
                    DraftEditorCandidateSessionRecordKeyV1::head(
                        opened.draft_id(),
                        opened.session_id(),
                    )
                };
                committed(execute(
                    &store,
                    inject_draft_candidate_publication_fault(
                        &store,
                        storage.clone(),
                        DraftCandidatePublicationFault::DeleteSessionRecord(key),
                    ),
                ));
            }
            4 => committed(execute(
                &store,
                delete_draft_edit_history_frontier(
                    &store,
                    storage.clone(),
                    opened.newest_history().key(),
                ),
            )),
            _ => {
                for key in [
                    DraftEditorCandidateSessionRecordKeyV1::head(
                        opened.draft_id(),
                        opened.session_id(),
                    ),
                    DraftEditorCandidateSessionRecordKeyV1::open_receipt(
                        opened.draft_id(),
                        opened.session_id(),
                        opened.open_operation_id(),
                    ),
                ] {
                    committed(execute(
                        &store,
                        inject_draft_candidate_publication_fault(
                            &store,
                            storage.clone(),
                            DraftCandidatePublicationFault::DeleteSessionRecord(key),
                        ),
                    ));
                }
            }
        }
        let revision = storage.revision(&store).unwrap();
        fail_home(&store, &faults);
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        for _ in 0..2 {
            assert!(
                fresh
                    .qualify_fresh_draft_editor_candidate_session_open_candidate(
                        &access,
                        home_id,
                        disposal_operation,
                        &original
                    )
                    .is_err()
            );
        }
        assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
        recovery.abort().close().unwrap();
    }
}

#[test]
fn candidate_disposal_collision_retains_exact_reconciliation_without_fabricating_disposal() {
    let disposal_operation = DraftPieceOperationIdV1::from_bytes([84; 16]);
    let (_home, store, storage, faults, thread) =
        fault_fixture("fresh-recovery-collision", 80, 65_536);
    let original = prepare_open(&storage, &store, thread, 82, 83);
    let home_id = store.home_id();
    let outcome = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            original.clone(),
        ),
    );
    let opened = match storage
        .reconcile_draft_editor_candidate_session_open(&store, &original, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
        other => panic!("opening unavailable: {other:?}"),
    };
    let request = abandon_request(&opened, 84);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let contribution = storage.abandon_fresh_draft_editor_candidate_session(
        storage.revision(&store).unwrap(),
        prepared.clone(),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = store.execute(command);
    assert!(matches!(outcome, CommandOutcome::Indeterminate { .. }));
    let (store, storage) = recover_if_failed(store, storage);
    committed(execute(
        &store,
        inject_draft_candidate_publication_fault(
            &store,
            storage.clone(),
            DraftCandidatePublicationFault::DeleteSessionRecord(
                DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
                    request.draft_id(),
                    request.session_id(),
                    request.operation_id(),
                ),
            ),
        ),
    ));
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = fresh.revision_candidate(&access).unwrap();
    assert!(matches!(
        fresh.reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
            &access, &prepared, outcome
        ),
        Err(DraftEditorCandidatePublicationCommandErrorV1::ReconciliationCollision)
    ));
    assert_eq!(access.pending_reconciliations().len(), 1);
    let handle = access.pending_reconciliations().pop().unwrap();
    assert_eq!(
        access.reconcile(&handle).unwrap(),
        beryl_home_store::ReconciliationResolution::Collision
    );
    assert_eq!(access.pending_reconciliations().len(), 1);
    assert!(
        fresh
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                home_id,
                disposal_operation,
                &original
            )
            .is_err()
    );
    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
    recovery.abort().close().unwrap();
}

#[test]
fn candidate_absence_rejects_orphan_receipt_at_the_retained_disposal_identity() {
    let (_home, store, storage, faults, thread) =
        fault_fixture("fresh-recovery-orphan-disposal", 90, 65_536);
    let original = prepare_open(&storage, &store, thread, 92, 93);
    let home_id = store.home_id();
    let outcome = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            original.clone(),
        ),
    );
    let opened = match storage
        .reconcile_draft_editor_candidate_session_open(&store, &original, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
        other => panic!("opening unavailable: {other:?}"),
    };
    let request = abandon_request(&opened, 94);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
    ));
    for key in [
        DraftEditorCandidateSessionRecordKeyV1::head(opened.draft_id(), opened.session_id()),
        DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            opened.draft_id(),
            opened.session_id(),
            opened.open_operation_id(),
        ),
    ] {
        committed(execute(
            &store,
            inject_draft_candidate_publication_fault(
                &store,
                storage.clone(),
                DraftCandidatePublicationFault::DeleteSessionRecord(key),
            ),
        ));
    }
    committed(execute(
        &store,
        delete_draft_edit_history_frontier(&store, storage.clone(), opened.newest_history().key()),
    ));
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    assert!(
        fresh
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                home_id,
                request.operation_id(),
                &original
            )
            .is_err()
    );
    recovery.abort().close().unwrap();
}
