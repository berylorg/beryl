use super::*;

#[cfg(feature = "test-faults")]
#[test]
fn indeterminate_first_acceptance_reconciles_exact_new_without_duplicate_delivery() {
    use beryl_home_store::test_faults::FaultPoint;

    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase175-indeterminate-new", 111);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 112, 113);
    let edited = commit_text(&mut host, &store, empty, 1, 0, 0, "once", 4, 1);
    let item = SyndicItemId::from_bytes([114; 16]);
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([115; 16]),
            item,
            DraftComposerMaterializationOperationIdV1::from_bytes([116; 16]),
            DraftPieceOperationIdV1::from_bytes([117; 16]),
            SyndicTimestamp::from_unix_millis(120),
            admission_requirement(),
        ))
        .unwrap();
    for _ in 0..128 {
        let outcome = host
            .advance_submission(
                &store,
                ticket,
                assets.clone(),
                &seals,
                operation_id(118),
                None,
                SyndicTimestamp::from_unix_millis(119),
                &CommandCancellation::new(),
            )
            .unwrap();
        if outcome
            == ComposerHostSubmissionAdvance::Progress(ComposerHostSubmissionStage::Accepting)
        {
            break;
        }
    }
    let injected = faults.clone();
    host.test_arm_submission_before_execute_fault(move |_, _| {
        injected.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    host.test_arm_submission_transition_fault(
        beryl_app::composer_host::ComposerHostSubmissionFaultPoint::AcceptanceAfterAttempt,
    );
    assert!(matches!(
        host.advance_submission(
            &store,
            ticket,
            assets.clone(),
            &seals,
            operation_id(118),
            None,
            SyndicTimestamp::from_unix_millis(119),
            &CommandCancellation::new(),
        )
        .unwrap_err(),
        beryl_app::composer_host::ComposerHostSubmissionError::InjectedFault(
            beryl_app::composer_host::ComposerHostSubmissionFaultPoint::AcceptanceAfterAttempt
        )
    ));
    assert!(host.submission_diagnostics().command_attempted());
    assert_eq!(
        host.advance_submission(
            &store,
            ticket,
            assets.clone(),
            &seals,
            operation_id(118),
            None,
            SyndicTimestamp::from_unix_millis(119),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle {
            user_item_id: item,
        })
    );
    assert_eq!(
        host.advance_submission(
            &store,
            ticket,
            assets.clone(),
            &seals,
            operation_id(118),
            None,
            SyndicTimestamp::from_unix_millis(119),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::Stale
    );
    assert!(
        storage
            .accepted_input(
                &store,
                edited.candidate().draft_id().accepted_input_id(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn definite_noncommit_preserves_the_draft_without_entering_reconciliation() {
    use beryl_home_store::{CommandOutcome, HomeCommand};
    use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};

    let (_home, mut store, storage, thread, _faults) =
        base::fault_fixture("phase175-exact-old", 121);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 122, 123);
    let edited = commit_text(&mut host, &store, empty, 1, 0, 0, "preserved", 4, 1);
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([124; 16]),
            SyndicItemId::from_bytes([125; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([126; 16]),
            DraftPieceOperationIdV1::from_bytes([127; 16]),
            SyndicTimestamp::from_unix_millis(130),
            admission_requirement(),
        ))
        .unwrap();
    advance_to_accepting(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        operation_id(128),
        SyndicTimestamp::from_unix_millis(129),
    );
    host.test_arm_submission_before_execute_fault(move |store, storage| {
        let record = storage
            .thread(store, thread, point_limit())
            .unwrap()
            .unwrap();
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::Thread(record)).unwrap();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    });
    assert_eq!(
        host.advance_submission(
            &store,
            ticket,
            assets.clone(),
            &seals,
            operation_id(128),
            None,
            SyndicTimestamp::from_unix_millis(129),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::NotCommitted
    );
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().id(), edited.candidate().draft_id());
    assert!(!host.submission_diagnostics().pending());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn definite_noncommit_releases_submission_custody_after_concurrent_draft_deletion() {
    use beryl_home_store::{CommandOutcome, HomeCommand};
    use syndic_storage::test_faults::{FixtureBatch, FixtureDelete};

    let (_home, mut store, storage, thread, _faults) =
        base::fault_fixture("phase175-collision", 131);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 132, 133);
    let edited = commit_text(&mut host, &store, empty, 1, 0, 0, "collision", 4, 1);
    let source_draft = edited.candidate().draft_id();
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([134; 16]),
            SyndicItemId::from_bytes([135; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([136; 16]),
            DraftPieceOperationIdV1::from_bytes([137; 16]),
            SyndicTimestamp::from_unix_millis(140),
            admission_requirement(),
        ))
        .unwrap();
    advance_to_accepting(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        operation_id(138),
        SyndicTimestamp::from_unix_millis(139),
    );
    host.test_arm_submission_before_execute_fault(move |store, storage| {
        let mut batch = FixtureBatch::new();
        batch.delete(FixtureDelete::Draft(source_draft)).unwrap();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    });
    assert_eq!(
        host.advance_submission(
            &store,
            ticket,
            assets.clone(),
            &seals,
            operation_id(138),
            None,
            SyndicTimestamp::from_unix_millis(139),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::NotCommitted
    );
    assert!(!host.submission_diagnostics().pending());
    assert!(
        storage
            .current_draft(&store, thread, point_limit())
            .is_err()
    );
}
