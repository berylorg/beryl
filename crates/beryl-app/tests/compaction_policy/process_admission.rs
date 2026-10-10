use super::*;
use beryl_app::cas_projection::CompactionCustodyTestStage;
use beryl_app::process_admission::ProcessAdmissionError;
use syndic_storage::{
    CompactionOperationState, CompactionRequestDisposition, CompactionSettlement,
};

#[test]
fn manual_compaction_admission_linearizes_with_fence_and_keeps_original_epoch() {
    for (stage, reopen_early) in [
        (CompactionCustodyTestStage::AdmissionCandidate, false),
        (CompactionCustodyTestStage::AdmissionCandidate, true),
        (CompactionCustodyTestStage::AdmissionReserved, false),
    ] {
        let reserved = matches!(stage, CompactionCustodyTestStage::AdmissionReserved);
        let mut fixture = Fixture::new(220);
        fixture.submit_text(SUBMITTED_TEXT);
        let server = CompactionServer::spawn(false);
        let (session, projection) = obtain(&fixture, &server);
        let retirement = session.connection_retirement_handle_for_test();
        let request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
        let outcome = thread::scope(|scope| {
            let worker = scope.spawn(|| execute(&fixture, projection, &request));
            server.wait_started();
            server.finish_turn();
            worker.join().unwrap().unwrap()
        });
        let OrdinaryTurnExecutionOutcome::Terminal { projection, .. } = outcome else {
            panic!("fixture must complete ordinary turn");
        };
        let harness = fixture
            .store
            .context_compaction_lifecycle_test_harness()
            .unwrap();
        let pause = harness.pause_compaction_custody(stage);
        let ready = reserved
            .then(|| harness.pause_compaction_custody(CompactionCustodyTestStage::AdmissionReady));
        thread::scope(|scope| {
            let pause = pause;
            let ready = ready;
            let worker = scope.spawn(|| {
                fixture
                    .store
                    .compact_thread(ContextCompactionRequest::new(fixture.thread, TIMEOUT))
            });
            pause.wait_until_paused();
            let revision = fixture.home().home_revision().unwrap();
            let fence = fixture.process_admission.test_fence().unwrap();
            let mut capture = reserved.then(|| {
                beryl_app::cas_projection::test_faults::ShutdownExecutionCaptureProbe::new(
                    &fixture.store,
                    &fence,
                )
                .unwrap()
            });
            if let Some(capture) = capture.as_mut() {
                capture.refresh(&fixture.store).unwrap();
                assert_eq!(capture.counts(), (0, 0));
            }
            if reserved {
                assert_eq!(
                    fence.try_reopen(true),
                    Err(ProcessAdmissionError::Unsettled)
                );
            } else if reopen_early {
                fence.try_reopen(true).unwrap();
            }
            pause.release();
            let captured_operation = ready.as_ref().map(|ready| {
                ready.wait_until_paused();
                let CompactionAdmissionRead::Existing(operation) = fixture
                    .storage
                    .compaction_admission_read(
                        &fixture.home(),
                        fixture.thread,
                        scheduler_support::point_limit(),
                    )
                    .unwrap()
                else {
                    panic!("winning manual admission must publish its exact operation");
                };
                let capture = capture.as_mut().unwrap();
                capture.refresh(&fixture.store).unwrap();
                assert_eq!(capture.counts(), (0, 1));
                assert_eq!(
                    capture
                        .compaction_settled(&fixture.store, operation.id())
                        .unwrap(),
                    Some(false)
                );
                ready.release();
                operation.id()
            });
            let result = worker.join().unwrap();
            if reserved {
                assert_eq!(result.unwrap(), ContextCompactionOutcome::Failed);
            } else {
                let expected = if reopen_early {
                    ProcessAdmissionError::Stale
                } else {
                    ProcessAdmissionError::Fenced
                };
                assert!(
                    matches!(result, Err(ContextCompactionError::ProcessAdmission(error)) if error == expected)
                );
                assert_eq!(fixture.home().home_revision().unwrap(), revision);
            }
            scheduler_support::wait_until("compaction admission cleanup", || {
                (harness.compaction_custody_in_use() == 0).then_some(())
            });
            assert!(!harness.has_local_compaction(fixture.thread));
            assert!(!retirement.is_retired());
            assert!(fixture.home().pending_reconciliations().is_empty());
            if let Some(operation) = captured_operation {
                let capture = capture.as_mut().unwrap();
                capture.refresh(&fixture.store).unwrap();
                assert_eq!(capture.counts(), (0, 1));
                assert_eq!(
                    capture
                        .compaction_settled(&fixture.store, operation)
                        .unwrap(),
                    Some(true)
                );
            }
            if !reopen_early {
                fence.try_reopen(true).unwrap();
            }
        });
        session.invalidate_connection();
        drop(projection);
        drop(session);
        server.join();
    }
}

#[test]
fn lifecycle_compaction_admission_uses_yield_epoch_across_fence_and_reopen() {
    for (stage, reopen_early) in [
        (CompactionCustodyTestStage::AdmissionCandidate, false),
        (CompactionCustodyTestStage::AdmissionCandidate, true),
        (CompactionCustodyTestStage::AdmissionReserved, false),
    ] {
        let reserved = matches!(stage, CompactionCustodyTestStage::AdmissionReserved);
        let mut fixture = Fixture::new(221);
        let submitted = fixture.submit_text(SUBMITTED_TEXT);
        let server = CompactionServer::spawn(false);
        let (session, projection) = obtain(&fixture, &server);
        let retirement = session.connection_retirement_handle_for_test();
        let harness = fixture
            .store
            .context_compaction_lifecycle_test_harness()
            .unwrap();
        let pause = harness.pause_compaction_custody(stage);
        let dispatch_pause = reserved
            .then(|| harness.pause_compaction_custody(CompactionCustodyTestStage::DispatchClaimed));
        let request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
        thread::scope(|scope| {
            let pause = pause;
            let dispatch_pause = dispatch_pause;
            eprintln!(
                "lifecycle admission fixture: {} reserved={reserved} reopen_early={reopen_early}",
                fixture.home_path().display()
            );
            let worker = scope.spawn(|| execute(&fixture, projection, &request));
            server.wait_started();
            scheduler_support::wait_until("accept continuation", || {
                fixture
                    .store
                    .record_lifecycle_yield_outcome(
                        fixture.thread,
                        submitted.turn,
                        LifecycleYieldOutcome::PhaseContinue,
                    )
                    .unwrap()
                    .then_some(())
            });
            server.finish_turn();
            pause.wait_until_paused();
            let revision = fixture.home().home_revision().unwrap();
            let fence = fixture.process_admission.test_fence().unwrap();
            if reserved {
                assert_eq!(
                    fence.try_reopen(true),
                    Err(ProcessAdmissionError::Unsettled)
                );
            } else if reopen_early {
                assert_eq!(
                    fence.try_reopen(true),
                    Err(ProcessAdmissionError::Unsettled)
                );
            }
            pause.release();
            let admitted = dispatch_pause.as_ref().map(|dispatch_pause| {
                dispatch_pause.wait_until_paused();
                let CompactionAdmissionRead::Existing(operation) = fixture
                    .storage
                    .compaction_admission_read(
                        &fixture.home(),
                        fixture.thread,
                        scheduler_support::point_limit(),
                    )
                    .unwrap()
                else {
                    panic!("winning lifecycle reservation must retain its dispatch claim");
                };
                dispatch_pause.release();
                operation
            });
            let outcome = worker.join().unwrap().unwrap();
            if reserved {
                assert!(matches!(
                    outcome,
                    OrdinaryTurnExecutionOutcome::LifecycleContinuationScheduled { .. }
                ));
            } else {
                assert!(matches!(
                    outcome,
                    OrdinaryTurnExecutionOutcome::Terminal { .. }
                ));
                assert_eq!(fixture.home().home_revision().unwrap(), revision);
            }
            scheduler_support::wait_until("lifecycle admission cleanup", || {
                (harness.compaction_custody_in_use() == 0).then_some(())
            });
            assert!(!harness.has_local_compaction(fixture.thread));
            if let Some(admitted) = admitted {
                let settled = fixture
                    .storage
                    .compaction_operation(
                        &fixture.home(),
                        admitted.id(),
                        scheduler_support::point_limit(),
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    settled.request().unwrap().disposition(),
                    CompactionRequestDisposition::ProvenLocalNondispatch
                );
                let CompactionOperationState::Consumed(witness) = settled.state() else {
                    panic!("winning reservation must settle without provider dispatch");
                };
                assert_eq!(
                    witness.settlement(),
                    &CompactionSettlement::LocalNondispatch
                );
            } else {
                assert!(!retirement.is_retired());
            }
            assert_eq!(
                fixture
                    .store
                    .take_terminal_lifecycle_yield_outcome(fixture.thread, submitted.turn)
                    .unwrap(),
                None
            );
            assert!(fixture.home().pending_reconciliations().is_empty());
            fence.try_reopen(true).unwrap();
        });
        session.invalidate_connection();
        drop(session);
        server.join();
    }
}

#[test]
fn process_fence_after_dispatch_claim_settles_local_nondispatch_and_releases_custody() {
    let mut fixture = Fixture::new(217);
    fixture.submit_text(SUBMITTED_TEXT);
    let server = CompactionServer::spawn(false);
    let (session, projection) = obtain(&fixture, &server);
    let retirement = session.connection_retirement_handle_for_test();
    let request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let outcome = thread::scope(|scope| {
        let worker = scope.spawn(|| execute(&fixture, projection, &request));
        server.wait_started();
        server.finish_turn();
        worker.join().unwrap().unwrap()
    });
    let OrdinaryTurnExecutionOutcome::Terminal { projection, .. } = outcome else {
        panic!("ordinary fixture must complete before compaction");
    };
    let harness = fixture
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let pause = harness.pause_compaction_custody(CompactionCustodyTestStage::DispatchClaimed);
    thread::scope(|scope| {
        let worker = scope.spawn(|| {
            fixture
                .store
                .compact_thread(ContextCompactionRequest::new(fixture.thread, TIMEOUT))
        });
        pause.wait_until_paused();
        let CompactionAdmissionRead::Existing(operation) = fixture
            .storage
            .compaction_admission_read(
                &fixture.home(),
                fixture.thread,
                scheduler_support::point_limit(),
            )
            .unwrap()
        else {
            panic!("compaction must retain its durable dispatch claim");
        };
        assert_eq!(
            operation.state(),
            &CompactionOperationState::DispatchClaimed
        );
        assert!(operation.dispatch_claim().is_some());
        assert_eq!(harness.compaction_custody_in_use(), 1);
        let fence = fixture.process_admission.test_fence().unwrap();
        pause.release();
        assert_eq!(
            worker.join().unwrap().unwrap(),
            ContextCompactionOutcome::Failed
        );
        scheduler_support::wait_until("local nondispatch driver releases custody", || {
            (harness.compaction_custody_in_use() == 0).then_some(())
        });
        assert!(!harness.has_local_compaction(fixture.thread));
        assert!(!retirement.is_retired());
        let home = fixture.home();
        let settled = fixture
            .storage
            .compaction_operation(&home, operation.id(), scheduler_support::point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(
            settled.request().unwrap().disposition(),
            CompactionRequestDisposition::ProvenLocalNondispatch
        );
        let CompactionOperationState::Consumed(witness) = settled.state() else {
            panic!("local nondispatch must consume the operation");
        };
        assert_eq!(
            witness.settlement(),
            &CompactionSettlement::LocalNondispatch
        );
        assert!(matches!(
            fixture
                .storage
                .compaction_admission_read(&home, fixture.thread, scheduler_support::point_limit())
                .unwrap(),
            CompactionAdmissionRead::Admissible(_)
        ));
        assert!(home.pending_reconciliations().is_empty());
        fence.try_reopen(true).unwrap();
    });
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}
