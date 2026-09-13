use super::*;
use beryl_app::cas_projection::CompactionCustodyTestStage;
use syndic_storage::{
    CompactionOperationState, CompactionRequestDisposition, CompactionSettlement,
};

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
