use super::*;

#[test]
fn admitted_command_settles_original_outcome_after_cancellation_and_keeps_one_worker() {
    let faults = FaultController::new();
    let fixture = Fixture::with_faults(faults.clone());
    history::create_inherited_discussion(&fixture, 1);
    let authority = fixture.service.selected_transcript_construction_authority();
    let overlapping = authority.clone();
    let cancellation = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancellation.clone();
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let worker = std::thread::spawn(move || {
        let result = authority.step(child(), &worker_cancel);
        (authority, result)
    });
    let reached = block.wait_until_reached(std::time::Duration::from_secs(10));
    if !reached {
        block.release();
    }
    assert!(reached);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 1);
    assert!(matches!(
        overlapping.step(child(), &AtomicBool::new(false)),
        Err(SelectedTranscriptConstructionError::Busy)
    ));
    cancellation.store(true, Ordering::Release);
    block.release();
    let (authority, result) = worker.join().unwrap();
    assert!(matches!(
        result,
        Err(SelectedTranscriptConstructionError::Cancelled)
    ));
    assert!(build(&fixture).is_some());
    authority.inspect_retained(|outcomes, terminal| {
        assert!(terminal.is_none());
        assert!(matches!(
            outcomes,
            [RetainedSelectedTranscriptOutcome::Command(
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            )]
        ));
    });
    quiescent(&fixture);
}

#[test]
fn committed_later_failure_closes_construction_and_retains_receipt_and_local_finalization_without_replay()
 {
    let faults = FaultController::new();
    let fixture = Fixture::with_faults(faults.clone());
    history::create_inherited_discussion(&fixture, 1);
    let authority = fixture.service.selected_transcript_construction_authority();
    faults.fail_next(FaultPoint::AfterPersist);
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)),
        Err(SelectedTranscriptConstructionError::CommandCommittedFailure)
    ));
    let original = authority.inspect_retained(|outcomes, terminal| {
        assert!(matches!(
            terminal,
            Some(SelectedTranscriptConstructionError::CommandCommittedFailure)
        ));
        let [
            RetainedSelectedTranscriptOutcome::Command(CommandOutcome::Committed {
                receipt,
                later_failure: Some(failure),
                local_finalization,
            }),
        ] = outcomes
        else {
            panic!("original committed failure and local-finalization custody")
        };
        assert!(local_finalization.is_some());
        assert!(!failure.to_string().is_empty());
        format!("{receipt:?}")
    });
    for _ in 0..2 {
        assert!(matches!(
            authority.step(child(), &AtomicBool::new(false)),
            Err(SelectedTranscriptConstructionError::CommandCommittedFailure)
        ));
        authority.inspect_retained(|outcomes, _| {
            let [
                RetainedSelectedTranscriptOutcome::Command(CommandOutcome::Committed {
                    receipt,
                    ..
                }),
            ] = outcomes
            else {
                panic!("retained original receipt")
            };
            assert_eq!(format!("{receipt:?}"), original);
        });
    }
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}

#[test]
fn indeterminate_exact_new_stops_the_job_with_original_classification_and_resumes_without_replay() {
    let faults = FaultController::new();
    let fixture = Fixture::with_faults(faults.clone());
    history::create_inherited_discussion(&fixture, 1);
    let authority = fixture.service.selected_transcript_construction_authority();
    let revision = fixture.home.home_revision().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)).unwrap(),
        SelectedTranscriptConstructionProgress::Pending
    ));
    assert_eq!(
        fixture.home.home_revision().unwrap().get(),
        revision.get() + 1
    );
    let classified = build(&fixture).unwrap();
    assert!(matches!(
        classified.phase(),
        TranscriptBuildPhase::Collecting { .. }
    ));
    authority.inspect_retained(|outcomes, terminal| {
        assert!(terminal.is_none());
        let [
            RetainedSelectedTranscriptOutcome::Reconciled {
                failure,
                resolution,
                ..
            },
        ] = outcomes
        else {
            panic!("one original uncertain outcome")
        };
        assert!(!failure.to_string().is_empty());
        assert!(matches!(
            resolution,
            Ok(beryl_home_store::ReconciliationResolution::ExactNew { .. })
        ));
    });
    assert!(fixture.home.pending_reconciliations().is_empty());
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)).unwrap(),
        SelectedTranscriptConstructionProgress::Current
    ));
    assert_eq!(
        build(&fixture).unwrap().generation(),
        classified.generation()
    );
    authority.inspect_retained(|outcomes, terminal| {
        assert!(terminal.is_none());
        assert!(outcomes.iter().all(|outcome| matches!(
            outcome,
            RetainedSelectedTranscriptOutcome::Command(CommandOutcome::Committed {
                later_failure: None,
                ..
            })
        )));
    });
    quiescent(&fixture);
}

#[test]
fn unresolved_reconciliation_retains_exact_handle_and_blocks_automatic_construction() {
    let faults = FaultController::new();
    let fixture = Fixture::with_faults(faults.clone());
    history::create_inherited_discussion(&fixture, 1);
    let authority = fixture.service.selected_transcript_construction_authority();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)),
        Err(SelectedTranscriptConstructionError::ReconciliationFailed)
    ));
    let original = authority.inspect_retained(|outcomes, terminal| {
        assert!(matches!(
            terminal,
            Some(SelectedTranscriptConstructionError::ReconciliationFailed)
        ));
        let [
            RetainedSelectedTranscriptOutcome::Reconciled {
                failure,
                handle,
                resolution: Err(_),
            },
        ] = outcomes
        else {
            panic!("unresolved original custody")
        };
        assert!(!failure.to_string().is_empty());
        format!("{handle:?}")
    });
    assert_eq!(fixture.home.pending_reconciliations().len(), 1);
    for _ in 0..2 {
        assert!(matches!(
            authority.step(child(), &AtomicBool::new(false)),
            Err(SelectedTranscriptConstructionError::ReconciliationFailed)
        ));
        authority.inspect_retained(|outcomes, _| {
            let [
                RetainedSelectedTranscriptOutcome::Reconciled {
                    handle,
                    resolution: Err(_),
                    ..
                },
            ] = outcomes
            else {
                panic!("unchanged unresolved custody")
            };
            assert_eq!(format!("{handle:?}"), original);
        });
    }
    assert_eq!(fixture.home.pending_reconciliations().len(), 1);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}

#[test]
fn definite_command_failure_is_retained_and_blocks_automatic_construction() {
    let faults = FaultController::new();
    let fixture = Fixture::with_faults(faults.clone());
    history::create_inherited_discussion(&fixture, 1);
    let authority = fixture.service.selected_transcript_construction_authority();
    faults.fail_next(FaultPoint::BeforeCommit);
    let result = authority.step(child(), &AtomicBool::new(false));
    assert!(matches!(
        result,
        Err(SelectedTranscriptConstructionError::CommandNotCommitted)
    ));
    authority.inspect_retained(|outcomes, terminal| {
        assert!(matches!(
            terminal,
            Some(SelectedTranscriptConstructionError::CommandNotCommitted)
        ));
        assert!(matches!(
            outcomes,
            [RetainedSelectedTranscriptOutcome::Command(
                CommandOutcome::NotCommitted { .. }
            )]
        ));
    });
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)),
        Err(SelectedTranscriptConstructionError::CommandNotCommitted)
    ));
    assert_eq!(retained_count(&authority), 1);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}
