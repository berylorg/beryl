use crate::{shared::*, support::*};

use beryl_home_store::ReconciliationResolution;
use syndic_storage::{
    DraftEditorCandidatePublicationCommandErrorV1,
    DraftEditorCandidateSessionAbandonFreshOutcomeV1, DraftEditorCandidateSessionRecordKeyV1,
    test_abandon_fresh_reconciliation_resolution,
    test_faults::{
        DraftCandidatePublicationFault, inject_draft_candidate_publication_fault,
        publish_draft_edit_history_pair,
    },
};

#[test]
fn abandonment_reconciles_every_atomic_fault_cut() {
    for (name, seed, fault, committed_at_cut) in [
        ("abandon-before-commit", 90, FaultPoint::BeforeCommit, false),
        (
            "abandon-after-commit",
            100,
            FaultPoint::AfterCommitBeforePersist,
            true,
        ),
        ("abandon-after-persist", 110, FaultPoint::AfterPersist, true),
        (
            "abandon-before-verification",
            120,
            FaultPoint::BeforeVerification,
            true,
        ),
    ] {
        let (home, store, storage, faults, thread) = fault_fixture(name, seed, 65_536);
        let selected = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &selected, seed + 2, seed + 3);
        let request = abandon_request(&opened, seed + 4);
        let prepared = storage
            .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
            .unwrap();
        faults.fail_next(fault);
        let outcome = execute(
            &store,
            storage.abandon_fresh_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        let (store, storage) = recover_if_failed(store, storage);
        let reconciled = storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome);
        if committed_at_cut {
            assert!(matches!(
                reconciled.unwrap(),
                DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
            ));
        } else {
            assert!(matches!(
                reconciled,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
            let retry = execute(
                &store,
                storage.abandon_fresh_draft_editor_candidate_session(
                    storage.revision(&store).unwrap(),
                    prepared.clone(),
                ),
            );
            assert!(matches!(
                storage
                    .reconcile_abandon_fresh_draft_editor_candidate_session(
                        &store, &prepared, retry,
                    )
                    .unwrap(),
                DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
            ));
        }
        let expected = head(&storage, &store, &opened);
        drop(store);
        let mut store = open(&home);
        let storage = SyndicStorage::register(&mut store).unwrap();
        assert_eq!(head(&storage, &store, &opened), expected);
        drop(home);
    }
}

#[test]
fn reconciliation_collision_is_typed_retained_and_does_not_fabricate_receipt() {
    let (home, store, storage, faults, thread) =
        fault_fixture("abandon-reconciliation-collision", 150, 65_536);
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 152, 153);
    let request = abandon_request(&opened, 154);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
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
    assert!(matches!(
        storage.reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome,),
        Err(DraftEditorCandidatePublicationCommandErrorV1::ReconciliationCollision)
    ));
    assert_eq!(store.pending_reconciliations().len(), 1);
    let retry = store.pending_reconciliations().pop().unwrap();
    assert_eq!(
        store.reconcile(&retry).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(store.pending_reconciliations().len(), 1);
    drop(retry);
    store.close().unwrap();

    let mut reopened = open(&home);
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(reopened.pending_reconciliations().is_empty());
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&reopened, request.draft_id(), request.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));
    let replay = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&reopened, request)
        .unwrap();
    let replay_outcome = execute(
        &reopened,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&reopened).unwrap(),
            replay.clone(),
        ),
    );
    assert!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(
                &reopened,
                &replay,
                replay_outcome,
            )
            .is_err()
    );
}

#[test]
fn collision_and_unauthorized_successor_are_distinct_terminal_classes() {
    assert!(matches!(
        test_abandon_fresh_reconciliation_resolution(ReconciliationResolution::Collision),
        Err(DraftEditorCandidatePublicationCommandErrorV1::ReconciliationCollision)
    ));

    let (_home, store, storage, thread) = fixture("abandon-resolution-classes", 160, 65_536);
    let selected = current(&storage, &store, thread);
    let receipt = match execute(
        &store,
        publish_draft_edit_history_pair(
            &store,
            storage,
            selected.draft().clone(),
            selected.draft().piece_root(),
            selected.draft().history(),
        ),
    ) {
        CommandOutcome::Committed { receipt, .. } => receipt,
        other => panic!("receipt fixture did not commit: {other:?}"),
    };
    assert!(matches!(
        test_abandon_fresh_reconciliation_resolution(ReconciliationResolution::ExactSuccessor {
            receipt
        }),
        Err(DraftEditorCandidatePublicationCommandErrorV1::UnauthorizedReconciliationSuccessor)
    ));
}
