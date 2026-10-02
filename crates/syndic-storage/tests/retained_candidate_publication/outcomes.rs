use super::*;

#[test]
fn retained_publication_correspondence_is_requalified_after_a_failed_fresh_save() {
    let (_home, store, storage, faults, thread) =
        fault_fixture("retained-older-retry", 150, 65_536);
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 152, 153);
    let first = commit_edit(&storage, &store, &opened, 154, "a");
    let original =
        prepare_candidate_publication(&storage, &store, &selected, first.adopted_session(), 155);
    let newer = commit_edit(&storage, &store, first.adopted_session(), 156, "b");
    let retained = DraftEditorCandidateActivationBindingV1::from_head(newer.adopted_session());
    committed(execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), original.clone()),
    ));
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let first_storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let correspondence = first_storage
        .qualify_retained_draft_editor_candidate_after_publication_candidate(
            &access, retained, &original,
        )
        .unwrap();
    let rebound = correspondence.candidate();
    let selected = correspondence.selector();
    let source = first_storage
        .capture_draft_editor_candidate_publication_source_candidate(
            &access,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                selected,
                rebound,
                DraftPieceOperationIdV1::from_bytes([157; 16]),
                SyndicTimestamp::from_unix_millis(4),
            ),
        )
        .unwrap();
    let save = first_storage
        .prepare_draft_editor_candidate_publication_candidate(
            &access,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap();
    let command = candidate_command(&first_storage, &access, &save);
    faults.fail_next(FaultPoint::BeforeCommit);
    let outcome = access.execute(command);
    let failed = recovery.abort();
    let mut recovery = failed.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    assert!(matches!(
        storage.reconcile_draft_editor_candidate_publication_candidate(&access, &save, outcome,),
        Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
    ));
    assert!(
        first_storage
            .qualify_retained_draft_editor_candidate_after_publication_candidate(
                &access, retained, &original,
            )
            .is_err()
    );
    assert!(
        storage
            .qualify_retained_draft_editor_candidate_after_publication_candidate(
                &access, rebound, &original,
            )
            .is_err()
    );
    let fresh = storage
        .qualify_retained_draft_editor_candidate_after_publication_candidate(
            &access, retained, &original,
        )
        .unwrap();
    assert_eq!(fresh.candidate(), rebound);
    assert_eq!(fresh.selector(), selected);
    assert_ne!(fresh.generation(), correspondence.generation());
    assert!(
        !storage
            .draft_editor_candidate_is_saved_candidate(&access, fresh.candidate(), fresh.selector())
            .unwrap()
    );
    let source = storage
        .capture_draft_editor_candidate_publication_source_candidate(
            &access,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                fresh.selector(),
                fresh.candidate(),
                DraftPieceOperationIdV1::from_bytes([158; 16]),
                SyndicTimestamp::from_unix_millis(5),
            ),
        )
        .unwrap();
    let retry = storage
        .prepare_draft_editor_candidate_publication_candidate(
            &access,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap();
    let outcome = access.execute(candidate_command(&storage, &access, &retry));
    storage
        .reconcile_draft_editor_candidate_publication_candidate(&access, &retry, outcome)
        .unwrap();
    storage
        .qualify_published_draft_editor_candidate_candidate(&access, fresh.candidate(), &retry)
        .unwrap();
    assert!(
        storage
            .qualify_retained_draft_editor_candidate_after_publication_candidate(
                &access, retained, &original,
            )
            .is_err()
    );
    recovery.publish().unwrap().close().unwrap();
}

#[test]
fn older_autosave_rebinds_the_exact_newer_unsaved_checkpoint_before_fresh_save() {
    for cut in [
        None,
        Some(FaultPoint::BeforeCommit),
        Some(FaultPoint::AfterCommitBeforePersist),
    ] {
        let (_home, store, storage, faults, thread) =
            fault_fixture("retained-older-autosave", 130, 65_536);
        let selected = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &selected, 132, 133);
        let first = commit_edit(&storage, &store, &opened, 134, "a");
        let prepared = prepare_candidate_publication(
            &storage,
            &store,
            &selected,
            first.adopted_session(),
            135,
        );
        let edit = transaction_with_positions(
            &storage,
            &store,
            first.adopted_session(),
            136,
            "b",
            point(1),
            point(1),
            point(2),
            point(2),
        );
        build(&storage, &store, &edit);
        committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        let settlement = settled(&storage, &store, &edit);
        let DraftPieceSettlementClosureV1::Committed(newer) = settlement.closure() else {
            panic!("newer edit did not commit");
        };
        let retained = DraftEditorCandidateActivationBindingV1::from_head(newer.adopted_session());
        if let Some(cut) = cut {
            faults.fail_next(cut);
        }
        let outcome = execute(
            &store,
            storage.publish_draft_editor_candidate(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        if store.health().state() == HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let settled = fresh
            .reconcile_draft_editor_candidate_publication_candidate(&access, &prepared, outcome);
        let (candidate, selector) = if cut == Some(FaultPoint::BeforeCommit) {
            assert!(matches!(
                settled,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
            assert!(matches!(
                fresh.qualify_retained_draft_editor_candidate_after_publication_candidate(
                    &access, retained, &prepared
                ),
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
            (retained, selector(&selected))
        } else {
            assert!(matches!(
                settled.unwrap(),
                DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
            ));
            assert!(
                fresh
                    .qualify_retained_draft_editor_candidate_after_publication_candidate(
                        &access,
                        DraftEditorCandidateActivationBindingV1::from_head(first.adopted_session()),
                        &prepared,
                    )
                    .is_err()
            );
            let correspondence = fresh
                .qualify_retained_draft_editor_candidate_after_publication_candidate(
                    &access, retained, &prepared,
                )
                .unwrap();
            assert_eq!(correspondence.home_id(), access.home_id());
            assert_eq!(correspondence.generation(), access.generation());
            let candidate = correspondence.candidate();
            assert_eq!(
                candidate.candidate_generation(),
                retained.candidate_generation()
            );
            assert_eq!(candidate.root(), retained.root());
            assert_eq!(candidate.history(), retained.history());
            assert_eq!(candidate.logical_extent(), retained.logical_extent());
            assert!(candidate.session_generation() > retained.session_generation());
            assert!(
                fresh
                    .draft_editor_candidate_is_saved_candidate(
                        &access,
                        retained,
                        correspondence.selector()
                    )
                    .is_err()
            );
            (candidate, correspondence.selector())
        };
        assert!(
            !fresh
                .draft_editor_candidate_is_saved_candidate(&access, candidate, selector)
                .unwrap()
        );
        let source = fresh
            .capture_draft_editor_candidate_publication_source_candidate(
                &access,
                DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                    selector,
                    candidate,
                    DraftPieceOperationIdV1::from_bytes([137; 16]),
                    SyndicTimestamp::from_unix_millis(4),
                ),
            )
            .unwrap();
        let save = fresh
            .prepare_draft_editor_candidate_publication_candidate(
                &access,
                source,
                DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
            )
            .unwrap();
        let outcome = access.execute(candidate_command(&fresh, &access, &save));
        assert!(matches!(
            fresh
                .reconcile_draft_editor_candidate_publication_candidate(&access, &save, outcome,)
                .unwrap(),
            DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
        ));
        let saved = fresh
            .qualify_published_draft_editor_candidate_candidate(&access, candidate, &save)
            .unwrap();
        assert_eq!(saved.candidate().root(), retained.root());
        assert_eq!(
            saved.candidate().candidate_generation(),
            retained.candidate_generation()
        );
        assert!(
            fresh
                .qualify_retained_draft_editor_candidate_after_publication_candidate(
                    &access, retained, &prepared
                )
                .is_err()
        );
        recovery.publish().unwrap().close().unwrap();
    }
}

#[test]
fn indeterminate_original_publication_keeps_exact_reconciliation_custody() {
    let (_home, store, storage, faults, thread) = fault_fixture("retained-uncertain", 90, 65_536);
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 92, 93);
    let edit = commit_edit(&storage, &store, &opened, 94, "uncertain publication");
    let candidate = edit.adopted_session();
    let prepared = prepare_candidate_publication(&storage, &store, &selected, candidate, 95);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), prepared.clone()),
    );
    assert!(matches!(outcome, CommandOutcome::Indeterminate { .. }));
    if store.health().state() == HomeHealthState::Healthy {
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
    }
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(matches!(
        fresh.reconcile_draft_editor_candidate_publication_candidate(&access, &prepared, outcome),
        Err(DraftEditorCandidatePublicationCommandErrorV1::Reconciliation(_))
    ));
    let pending = access.pending_reconciliations();
    assert_eq!(pending.len(), 1);
    let mut recovery = recovery.publish().unwrap_err().into_parts().1;
    let access = recovery.recovery_access().unwrap();
    assert!(matches!(
        access.retry_reconciliation(&pending[0]).unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    let correspondence = fresh
        .qualify_published_draft_editor_candidate_candidate(
            &access,
            DraftEditorCandidateActivationBindingV1::from_head(candidate),
            &prepared,
        )
        .unwrap();
    assert_eq!(correspondence.candidate().root(), candidate.newest_root());
    assert!(access.pending_reconciliations().is_empty());
    recovery.publish().unwrap().close().unwrap();
}

#[test]
fn fresh_publication_faults_keep_outcomes_separate_across_another_recovery() {
    for (cut, committed_at_cut) in [
        (FaultPoint::BeforeCommit, false),
        (FaultPoint::AfterCommitBeforePersist, true),
        (FaultPoint::AfterPersist, true),
        (FaultPoint::BeforeVerification, true),
    ] {
        let (_home, store, storage, faults, thread) =
            fault_fixture("retained-fresh-fault", 110, 65_536);
        let selected = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &selected, 112, 113);
        let edit = commit_edit(&storage, &store, &opened, 114, "fresh save retained");
        let candidate = edit.adopted_session();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let prepared = prepare_candidate(
            &fresh,
            &access,
            capture_request(selector(&selected), candidate, 115),
        );
        let command = candidate_command(&fresh, &access, &prepared);
        faults.fail_next(cut);
        let outcome = access.execute(command);
        let failed = recovery.abort();
        let mut recovery = failed.recover_same_home().unwrap();
        let newer = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let result = newer
            .reconcile_draft_editor_candidate_publication_candidate(&access, &prepared, outcome);
        if committed_at_cut {
            assert!(matches!(
                result.unwrap(),
                DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
            ));
            newer
                .qualify_published_draft_editor_candidate_candidate(
                    &access,
                    DraftEditorCandidateActivationBindingV1::from_head(candidate),
                    &prepared,
                )
                .unwrap();
        } else {
            assert!(matches!(
                result,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
            assert!(
                !newer
                    .draft_editor_candidate_is_saved_candidate(
                        &access,
                        DraftEditorCandidateActivationBindingV1::from_head(candidate),
                        selector(&selected)
                    )
                    .unwrap()
            );
        }
        assert!(
            fresh
                .capture_draft_editor_candidate_publication_source_candidate(
                    &access,
                    capture_request(selector(&selected), candidate, 116)
                )
                .is_err()
        );
        recovery.publish().unwrap().close().unwrap();
    }
}
