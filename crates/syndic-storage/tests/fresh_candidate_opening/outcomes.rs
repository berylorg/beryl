use super::*;

#[test]
fn fresh_candidate_opening_preserves_actual_noncommit_and_ambiguous_outcome_across_recovery() {
    for cut in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
        FaultPoint::BeforeVerification,
    ] {
        let (_home, store, _storage, faults, thread) =
            fault_fixture("fresh-candidate-cuts", 50, 65_536);
        fail_home(&store, &faults);
        let mut recovery = store.recover_same_home().unwrap();
        let first = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let selected = first
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
            DraftEditorCandidateSessionIdV1::from_bytes([52; 16]),
            DraftPieceOperationIdV1::from_bytes([53; 16]),
        );
        let prepared = first
            .prepare_open_draft_editor_candidate_session_candidate(&access, request)
            .unwrap();
        let mut command = HomeCommand::new(access.home_revision().unwrap());
        command
            .add(first.open_draft_editor_candidate_session(
                first.revision_candidate(&access).unwrap(),
                prepared.clone(),
            ))
            .unwrap();
        faults.fail_next(cut);
        let outcome = access.execute(command);
        let failed = recovery.abort();
        let mut recovery = failed.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        if cut == FaultPoint::BeforeCommit {
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
                        DraftPieceOperationIdV1::from_bytes([54; 16]),
                        &prepared,
                    )
                    .unwrap(),
                DraftEditorCandidateSessionReadOutcomeV1::Absent
            ));
            let retry = candidate_execute(
                &access,
                fresh.open_draft_editor_candidate_session(
                    fresh.revision_candidate(&access).unwrap(),
                    prepared.clone(),
                ),
            );
            assert!(matches!(
                fresh
                    .reconcile_fresh_draft_editor_candidate_session_open_candidate(
                        &access, &prepared, retry
                    )
                    .unwrap(),
                DraftEditorCandidateSessionOpenOutcomeV1::Opened(_)
            ));
        } else {
            assert!(matches!(
                fresh
                    .reconcile_fresh_draft_editor_candidate_session_open_candidate(
                        &access, &prepared, outcome
                    )
                    .unwrap(),
                DraftEditorCandidateSessionOpenOutcomeV1::Opened(_)
            ));
            assert!(access.pending_reconciliations().is_empty());
            assert!(
                fresh
                    .prepare_open_draft_editor_candidate_session_candidate(&access, request)
                    .is_err()
            );
        }
        recovery.publish().unwrap().close().unwrap();
    }
}
