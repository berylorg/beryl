use super::*;

#[test]
fn candidate_begin_and_page_commands_reconcile_exact_old_or_new_across_commit_cuts() {
    for advance in [false, true] {
        for cut in [
            FaultPoint::BeforeCommit,
            FaultPoint::AfterCommitBeforePersist,
            FaultPoint::AfterPersist,
            FaultPoint::BeforeVerification,
        ] {
            let faults = FaultController::new();
            let (_home, store, storage, thread) =
                fixture_with_faults("candidate-seal-cuts", 50, faults.clone());
            let source = current(&storage, &store, thread).draft().piece_root();
            let request = DraftMarkerSealRequestV1::new(
                source,
                DraftMarkerSealOperationIdV1::from_bytes([52; 16]),
            );
            if advance {
                let begin = storage
                    .prepare_draft_marker_seal_begin(&store, request)
                    .unwrap();
                committed(execute(
                    &store,
                    storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
                ));
            }
            fail_home(&store, &faults);
            let mut recovery = store.recover_same_home().unwrap();
            let first = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            let access = recovery.recovery_access().unwrap();
            let contribution = if advance {
                let page = first
                    .prepare_draft_marker_seal_advance_candidate(&access, request.key())
                    .unwrap()
                    .unwrap();
                assert!(page.page().exact_eof());
                first.advance_draft_marker_seal(first.revision_candidate(&access).unwrap(), &page)
            } else {
                let begin = first
                    .prepare_draft_marker_seal_begin_candidate(&access, request)
                    .unwrap();
                first.begin_draft_marker_seal(first.revision_candidate(&access).unwrap(), begin)
            };
            let mut command = HomeCommand::new(access.home_revision().unwrap());
            command.add(contribution).unwrap();
            faults.fail_next(cut);
            let outcome = access.execute(command);
            let failed = recovery.abort();
            let mut recovery = failed.recover_same_home().unwrap();
            let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            let access = recovery.recovery_access().unwrap();
            let committed_at_cut = settle_candidate(&access, outcome);
            assert_eq!(committed_at_cut, cut != FaultPoint::BeforeCommit);
            let status = fresh
                .draft_marker_seal_status_candidate(&access, request.key())
                .unwrap();
            assert!(matches!(
                (advance, committed_at_cut, status),
                (false, false, DraftMarkerSealStatusV1::Absent)
                    | (
                        false,
                        true,
                        DraftMarkerSealStatusV1::Open {
                            completed_marker_count: 0
                        }
                    )
                    | (
                        true,
                        false,
                        DraftMarkerSealStatusV1::Open {
                            completed_marker_count: 0
                        }
                    )
                    | (true, true, DraftMarkerSealStatusV1::Sealed(_, _))
            ));
            assert!(
                first
                    .draft_marker_seal_status_candidate(&access, request.key())
                    .is_err()
            );
            if !committed_at_cut {
                let contribution = if advance {
                    let page = fresh
                        .prepare_draft_marker_seal_advance_candidate(&access, request.key())
                        .unwrap()
                        .unwrap();
                    fresh.advance_draft_marker_seal(
                        fresh.revision_candidate(&access).unwrap(),
                        &page,
                    )
                } else {
                    fresh.begin_draft_marker_seal(
                        fresh.revision_candidate(&access).unwrap(),
                        fresh
                            .prepare_draft_marker_seal_begin_candidate(&access, request)
                            .unwrap(),
                    )
                };
                committed(candidate_execute(&access, contribution));
            }
            recovery.publish().unwrap().close().unwrap();
        }
    }
}

#[test]
fn unresolved_marker_command_keeps_reconciliation_custody_until_exact_retry() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("candidate-seal-pending", 60, faults.clone());
    let source = current(&storage, &store, thread).draft().piece_root();
    let request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([62; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = execute(
        &store,
        storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin),
    );
    let CommandOutcome::Indeterminate { reconciliation, .. } = outcome else {
        panic!("expected uncertain seal begin");
    };
    let handle = reconciliation.install_and_handle();
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(access.reconcile(&handle).is_err());
    assert_eq!(access.pending_reconciliations().len(), 1);
    let mut recovery = recovery.publish().unwrap_err().into_parts().1;
    let access = recovery.recovery_access().unwrap();
    assert!(matches!(
        access.retry_reconciliation(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(access.pending_reconciliations().is_empty());
    assert!(matches!(
        fresh
            .draft_marker_seal_status_candidate(&access, request.key())
            .unwrap(),
        DraftMarkerSealStatusV1::Open {
            completed_marker_count: 0
        }
    ));
    recovery.publish().unwrap().close().unwrap();
}
