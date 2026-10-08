use super::{edit_support::commit_edit, publication_support::*, support::*};
use syndic_storage::{
    DraftEditorCandidatePublicationCommandErrorV1, DraftEditorCandidateSessionDisposeOutcomeV1,
    DraftEditorCandidateSessionDisposeRequestV1, DraftRootHistoryPairV1,
};

fn request(
    head: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> DraftEditorCandidateSessionDisposeRequestV1 {
    DraftEditorCandidateSessionDisposeRequestV1::new(
        head.draft_id(),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        head.session_generation(),
        DraftRootHistoryPairV1::new(head.published_root(), head.published_history()),
    )
}

#[test]
fn candidate_disposal_rejects_foreign_prepared_intent_with_identical_natural_keys() {
    let (_home, store, storage, faults, thread) = fault_fixture("disposal-provenance", 160, 65_536);
    let opened = open_session(
        &storage,
        &store,
        &current(&storage, &store, thread),
        162,
        163,
    );
    let intent = request(&opened, 164);
    let (_foreign_home, foreign_store, foreign, foreign_thread) =
        fixture("foreign-disposal-provenance", 160, 65_536);
    let foreign_opened = open_session(
        &foreign,
        &foreign_store,
        &current(&foreign, &foreign_store, foreign_thread),
        162,
        163,
    );
    assert_eq!(request(&foreign_opened, 164), intent);
    let prepared = foreign
        .prepare_dispose_draft_editor_candidate_session(&foreign_store, intent)
        .unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    assert!(
        fresh
            .dispose_draft_editor_candidate_session_candidate(
                &access,
                fresh.revision_candidate(&access).unwrap(),
                prepared
            )
            .is_err()
    );
    assert_eq!(access.home_revision().unwrap(), revision);
    let store = recovery.publish().unwrap();
    assert_eq!(head(&fresh, &store, &opened), opened);
}

#[test]
fn candidate_disposal_settles_original_cuts_before_remaining_cleanup_and_never_repeats_commit() {
    for cut in [
        None,
        Some(FaultPoint::BeforeCommit),
        Some(FaultPoint::AfterCommitBeforePersist),
        Some(FaultPoint::BeforeVerification),
    ] {
        let (_home, store, storage, faults, thread) =
            fault_fixture("candidate-disposal-cut", 100, 65_536);
        let durable = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &durable, 102, 103);
        let request = request(&opened, 104);
        let original = storage
            .prepare_dispose_draft_editor_candidate_session(&store, request)
            .unwrap();
        if let Some(cut) = cut {
            faults.fail_next(cut);
        }
        let outcome = execute(
            &store,
            storage.dispose_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                original.clone(),
            ),
        );
        if store.home_revision().is_ok() {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let revision = access.home_revision().unwrap();
        assert!(
            storage
                .prepare_dispose_draft_editor_candidate_session_candidate(&access, request)
                .is_err()
        );
        let (_foreign_home, _foreign_store, foreign, _) = fixture("foreign-disposal", 110, 65_536);
        assert!(
            foreign
                .prepare_dispose_draft_editor_candidate_session_candidate(&access, request)
                .is_err()
        );
        let settled = fresh.reconcile_draft_editor_candidate_session_disposal_candidate(
            &access, &original, outcome,
        );
        assert_eq!(access.home_revision().unwrap(), revision);
        if cut == Some(FaultPoint::BeforeCommit) {
            assert!(matches!(
                settled,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
            let remaining = fresh
                .prepare_dispose_draft_editor_candidate_session_candidate(&access, request)
                .unwrap();
            let contribution = fresh
                .dispose_draft_editor_candidate_session_candidate(
                    &access,
                    fresh.revision_candidate(&access).unwrap(),
                    remaining.clone(),
                )
                .unwrap();
            let mut command = HomeCommand::new(access.home_revision().unwrap());
            command.add(contribution).unwrap();
            let outcome = access.execute(command);
            assert!(matches!(
                fresh
                    .reconcile_draft_editor_candidate_session_disposal_candidate(
                        &access, &remaining, outcome
                    )
                    .unwrap(),
                DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
            ));
        } else {
            assert!(matches!(
                settled.unwrap(),
                DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
            ));
            let validated = fresh
                .prepare_dispose_draft_editor_candidate_session_candidate(&access, request)
                .unwrap();
            assert_eq!(validated.request(), request);
            assert_eq!(access.home_revision().unwrap(), revision);
        }
        let store = recovery.publish().unwrap();
        let terminal = head(&fresh, &store, &opened);
        assert_eq!(terminal.newest_root(), terminal.published_root());
        assert_eq!(terminal.newest_history(), terminal.published_history());
        assert_eq!(current(&fresh, &store, thread), durable);
    }
}

#[test]
fn candidate_disposal_refuses_dirty_stale_and_substituted_session_facts() {
    for changed in 0..3 {
        let (_home, store, storage, faults, thread) =
            fault_fixture("candidate-disposal-invalid", 120, 65_536);
        let durable = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &durable, 122, 123);
        let selected = if changed == 0 {
            commit_edit(&storage, &store, &opened, 124, "unsaved")
                .adopted_session()
                .clone()
        } else {
            opened.clone()
        };
        let intent = if changed == 2 {
            DraftEditorCandidateSessionDisposeRequestV1::new(
                selected.draft_id(),
                DraftEditorCandidateSessionIdV1::from_bytes([200; 16]),
                DraftPieceOperationIdV1::from_bytes([125; 16]),
                selected.session_generation(),
                DraftRootHistoryPairV1::new(
                    selected.published_root(),
                    selected.published_history(),
                ),
            )
        } else if changed == 1 {
            DraftEditorCandidateSessionDisposeRequestV1::new(
                selected.draft_id(),
                selected.session_id(),
                DraftPieceOperationIdV1::from_bytes([125; 16]),
                selected.session_generation() + 1,
                DraftRootHistoryPairV1::new(
                    selected.published_root(),
                    selected.published_history(),
                ),
            )
        } else {
            request(&selected, 125)
        };
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let revision = access.home_revision().unwrap();
        if let Ok(prepared) =
            fresh.prepare_dispose_draft_editor_candidate_session_candidate(&access, intent)
        {
            let contribution = fresh
                .dispose_draft_editor_candidate_session_candidate(
                    &access,
                    fresh.revision_candidate(&access).unwrap(),
                    prepared,
                )
                .unwrap();
            let mut command = HomeCommand::new(revision);
            command.add(contribution).unwrap();
            assert!(matches!(
                access.execute(command),
                CommandOutcome::NotCommitted { .. }
            ));
        }
        assert_eq!(access.home_revision().unwrap(), revision);
        let store = recovery.publish().unwrap();
        assert_eq!(head(&fresh, &store, &selected), selected);
    }
}
