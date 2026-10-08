use super::*;

#[test]
fn fresh_candidate_cancelled_opening_retains_absent_original_intent() {
    let (_home, store, _storage, faults, thread) =
        fault_fixture("fresh-candidate-cancel", 90, 65_536);
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let selected = fresh
        .current_draft_piece_text_demand_candidate(
            &access,
            thread,
            DraftPieceTextDemandV1::Validate(0),
            4,
        )
        .unwrap()
        .unwrap()
        .selector();
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selected,
        DraftEditorCandidateSessionIdV1::from_bytes([92; 16]),
        DraftPieceOperationIdV1::from_bytes([93; 16]),
    );
    let prepared = fresh
        .prepare_open_draft_editor_candidate_session_candidate(&access, request)
        .unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let revision = fresh.revision_candidate(&access).unwrap();
    let mut command =
        HomeCommand::new(access.home_revision().unwrap()).with_cancellation(cancellation);
    command
        .add(fresh.open_draft_editor_candidate_session(revision, prepared.clone()))
        .unwrap();
    let outcome = access.execute(command);
    assert!(matches!(&outcome, CommandOutcome::NotCommitted { .. }));
    assert!(
        fresh
            .reconcile_fresh_draft_editor_candidate_session_open_candidate(
                &access, &prepared, outcome
            )
            .is_err()
    );
    assert!(matches!(
        fresh
            .qualify_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                access.home_id(),
                DraftPieceOperationIdV1::from_bytes([94; 16]),
                &prepared,
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Absent
    ));
    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
    recovery.publish().unwrap().close().unwrap();
}
