use super::execution_lifetime::{release, started, turn};
use super::*;
use beryl_app::{
    LifecycleYieldOutcome,
    cas_projection::{
        CompactionWorkPageLimits, ProcessWorkPageLimits, ProjectionCancellationToken,
        RuntimeInterestKind,
    },
};
use syndic_storage::TurnLifecycle;

#[test]
fn managed_lifecycle_compaction_runs_continuation_after_retirement_without_views() {
    let (mut fixture, sessions, attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-compaction").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let readiness = support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let first = started(&fixture, &sessions, 0);
    let process = ProcessWitness::open(first["pid"].as_u64().unwrap() as u32);
    let first_turn = turn(&fixture, TurnLifecycle::Active);
    let harness = fixture
        .service()
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let settlement = harness.pause_after_lifecycle_settlement().unwrap();
    assert!(
        fixture
            .service()
            .record_lifecycle_yield_outcome(
                thread_id(1),
                first_turn,
                LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );
    drop(view);
    release(&fixture, 0);
    wait_until(|| fixture.root(1).join("compaction-requested").exists());
    let live = fixture.service().live_home_command().unwrap();
    let syndic_storage::CompactionAdmissionRead::Existing(operation) = fixture
        .storage
        .compaction_admission_read(
            live.home(),
            thread_id(1),
            syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
    else {
        panic!("managed compaction must own its operation");
    };
    let operation_id = operation.id();
    drop(live);
    assert!(
        process.running(),
        "process exited before any compaction provider observation: {:?}",
        fixture
            .storage
            .compaction_operation(
                fixture.service().live_home_command().unwrap().home(),
                operation_id,
                syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()
            )
            .unwrap()
    );
    fs::write(fixture.root(1).join("compaction-begin"), "ready").unwrap();
    wait_until(|| fixture.root(1).join("compaction-started").exists());
    assert!(process.running());
    assert_eq!(sessions.diagnostics().retained, 1);
    let inventory = fixture
        .service()
        .process_work_inventory(&sessions, &attention);
    wait_until(|| {
        let page = inventory.revision().and_then(|revision| {
            inventory.page(
                &revision,
                None,
                ProcessWorkPageLimits::new(256, 65_536).unwrap(),
                &ProjectionCancellationToken::new(),
            )
        });
        match page {
            Ok(page) => {
                assert_eq!(
                    page.total_threads(),
                    1,
                    "operation {:?}",
                    fixture
                        .storage
                        .compaction_operation(
                            fixture.service().live_home_command().unwrap().home(),
                            operation_id,
                            syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()
                        )
                        .unwrap()
                );
                page.records()[0].facts.compacting && page.records()[0].facts.continuation
            }
            Err(beryl_app::cas_projection::ProcessWorkError::StaleRevision)
            | Err(beryl_app::cas_projection::ProcessWorkError::Connections(
                beryl_app::cas_projection::ConnectionWorkError::StaleRevision,
            )) => false,
            Err(error) => panic!("cannot read process work: {error:?}"),
        }
    });
    drop(inventory);
    let reattached = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert_eq!(support::ready(&reattached), readiness);
    assert!(!fixture.root(1).join("execution-started-1.json").exists());
    drop(reattached);
    fs::write(fixture.root(1).join("compaction-release"), "ready").unwrap();
    settlement.wait_until_settled();
    assert!(process.running());
    assert!(!fixture.root(1).join("execution-started-1.json").exists());
    assert_eq!(harness.compaction_custody_in_use(), 1);
    {
        let live = fixture.service().live_home_command().unwrap();
        let limit = syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap();
        let settled = fixture
            .storage
            .compaction_operation(live.home(), operation_id, limit)
            .unwrap()
            .unwrap();
        assert!(
            matches!(settled.state(), syndic_storage::CompactionOperationState::Consumed(witness)
            if matches!(witness.settlement(), syndic_storage::CompactionSettlement::LifecycleContinuation { .. }))
        );
        assert!(matches!(
            fixture
                .storage
                .input_gate(live.home(), thread_id(1), limit)
                .unwrap()
                .unwrap()
                .state(),
            syndic_storage::InputGateState::PendingTurn(_)
        ));
    }
    settlement.release();
    let second = started(&fixture, &sessions, 1);
    process.assert_exited();
    assert_eq!(second["thread"], first["thread"]);
    assert_eq!(second["text"], syndic_storage::LIFECYCLE_CONTINUATION_TEXT);
    let successor_process = ProcessWitness::open(second["pid"].as_u64().unwrap() as u32);
    let second_turn = turn(&fixture, TurnLifecycle::Active);
    assert_ne!(second_turn, first_turn);
    wait_until(|| harness.compaction_custody_in_use() == 0);
    let revision = fixture.service().compaction_work_revision().unwrap();
    assert!(
        fixture
            .service()
            .compaction_work_page(
                &revision,
                None,
                CompactionWorkPageLimits::new(80, 65_536).unwrap()
            )
            .unwrap()
            .records()
            .is_empty()
    );
    release(&fixture, 1);
    assert_eq!(turn(&fixture, TurnLifecycle::Complete), second_turn);
    process.assert_exited();
    successor_process.assert_exited();
    wait_until(|| {
        sessions.diagnostics().retained == 0
            && fixture.token_count() == 0
            && fixture.service().worker_pool_diagnostics().active() == 0
    });
    assert!(
        !fixture
            .service()
            .accepted_input_scheduler_diagnostics()
            .fatal()
    );
    {
        let live = fixture.service().live_home_command().unwrap();
        let limit = syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap();
        let successor = fixture
            .storage
            .turn(live.home(), second_turn, limit)
            .unwrap()
            .unwrap();
        assert_eq!(
            successor.kind(),
            syndic_storage::TurnKind::BerylLifecycleContinuation
        );
        let state = fixture
            .storage
            .turn_state(live.home(), second_turn, limit)
            .unwrap()
            .unwrap();
        assert_eq!(state.lifecycle(), TurnLifecycle::Complete);
        assert_eq!(state.item_count(), state.finalized_item_count());
        assert_eq!(state.open_item_count(), 0);
        assert_eq!(state.history_blocking_item_count(), 0);
        assert_eq!(
            fixture
                .storage
                .input_gate(live.home(), thread_id(1), limit)
                .unwrap()
                .unwrap()
                .state(),
            &syndic_storage::InputGateState::Idle
        );
    }
    drop(harness);
    close(&mut fixture, &sessions);
}
