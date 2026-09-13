use super::*;
use beryl_app::process_admission::ProcessAdmissionError;

#[test]
fn process_fence_rejects_new_continuation_intent_but_allows_terminal_yield() {
    let mut fixture = syndic::Fixture::new(222);
    let submitted = fixture.submit_text(" shutdown continuation admission");
    let fence = fixture.process_admission.test_fence().unwrap();
    assert!(
        !fixture
            .store
            .record_lifecycle_yield_outcome(
                fixture.thread,
                submitted.turn,
                beryl_app::LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    assert!(
        fixture
            .store
            .record_lifecycle_yield_outcome(
                fixture.thread,
                submitted.turn,
                beryl_app::LifecycleYieldOutcome::PlanComplete
            )
            .unwrap()
    );
    fence.try_reopen(true).unwrap();
}

#[test]
fn indeterminate_continuation_keeps_reservation_until_exact_reconciliation_behind_fence() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};
    let faults = FaultController::new();
    let fixture = LifecycleFixture::with_faults(223, 244, faults.clone());
    fixture.publish_success_prefix();
    let pause = fixture.harness.pause_after_lifecycle_settlement().unwrap();
    faults.fail_next_in_scope(
        FaultPoint::AfterCommitBeforePersist,
        syndic_storage::test_faults::lifecycle_compaction_settlement_fault_scope(),
    );
    thread::scope(|scope| {
        let terminal = scope.spawn(|| fixture.publish_success_terminal());
        pause.wait_until_settled();
        let fence = fixture.process_admission.test_fence().unwrap();
        assert_eq!(
            fence.try_reopen(true),
            Err(ProcessAdmissionError::Unsettled)
        );
        pause.release();
        terminal.join().unwrap();
        let operation = fixture.operation();
        let CompactionOperationState::Consumed(witness) = operation.state() else {
            panic!("exact reconciliation must prove continuation settlement");
        };
        assert!(matches!(
            witness.settlement(),
            CompactionSettlement::LifecycleContinuation { .. }
        ));
        let live = fixture.service.live_home_command().unwrap();
        let state = fixture
            .storage
            .turn_state(
                live.home(),
                fixture.committed_tail().unwrap(),
                point_limit(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(state.lifecycle(), syndic_storage::TurnLifecycle::Pending);
        assert_eq!(state.source_event_count(), 0);
        assert!(live.home().pending_reconciliations().is_empty());
        drop(live);
        fence.try_reopen(true).unwrap();
    });
    fixture.close();
}

#[test]
fn fence_at_continuation_cut_preserves_accepted_input_and_does_not_revive_intent() {
    for reopen_before_settlement in [false, true] {
        let fixture = LifecycleFixture::with_accepted_next(218, 240);
        let original_tail = fixture.committed_tail();
        fixture.publish_success_prefix();
        let pause = fixture.harness.pause_after_lifecycle_staging().unwrap();
        thread::scope(|scope| {
            let terminal = scope.spawn(|| fixture.publish_success_terminal());
            pause.wait_until_staged();
            let fence = fixture.process_admission.test_fence().unwrap();
            if reopen_before_settlement {
                fence.try_reopen(true).unwrap();
            }
            pause.release();
            terminal.join().unwrap();
            if !reopen_before_settlement {
                fence.try_reopen(true).unwrap();
            }
        });
        let operation = fixture.operation();
        let CompactionOperationState::Consumed(witness) = operation.state() else {
            panic!("fenced continuation must settle successful compaction");
        };
        assert_eq!(witness.settlement(), &CompactionSettlement::ManualSuccess);
        assert_eq!(fixture.committed_tail(), original_tail);
        let live = fixture.service.live_home_command().unwrap();
        let gate = fixture
            .storage
            .input_gate(live.home(), fixture.thread_id, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(gate.live_next_turn_count(), 1);
        assert_eq!(gate.state(), &syndic_storage::InputGateState::Idle);
        assert!(live.home().pending_reconciliations().is_empty());
        drop(live);
        assert_eq!(
            fixture
                .service
                .take_terminal_lifecycle_yield_outcome(fixture.thread_id, fixture.yielding_turn_id)
                .unwrap(),
            None
        );
        fixture.close();
    }
}

#[test]
fn fence_after_continuation_publication_preserves_exact_pending_content_until_settlement() {
    let fixture = LifecycleFixture::new(219, 242);
    fixture.publish_success_prefix();
    let pause = fixture.harness.pause_after_lifecycle_settlement().unwrap();
    thread::scope(|scope| {
        let terminal = scope.spawn(|| fixture.publish_success_terminal());
        pause.wait_until_settled();
        let pending = fixture.committed_tail();
        let before = {
            let live = fixture.service.live_home_command().unwrap();
            fixture
                .storage
                .turn(live.home(), pending.unwrap(), point_limit())
                .unwrap()
                .unwrap()
        };
        let operation = fixture.operation();
        let CompactionOperationState::Consumed(witness) = operation.state() else {
            panic!("continuation publication must consume the operation");
        };
        assert!(matches!(
            witness.settlement(),
            CompactionSettlement::LifecycleContinuation { .. }
        ));
        let fence = fixture.process_admission.test_fence().unwrap();
        assert_eq!(
            fence.try_reopen(true),
            Err(ProcessAdmissionError::Unsettled)
        );
        pause.release();
        terminal.join().unwrap();
        assert_eq!(fixture.committed_tail(), pending);
        assert_eq!(fixture.operation(), operation);
        let live = fixture.service.live_home_command().unwrap();
        assert_eq!(
            fixture
                .storage
                .turn(live.home(), pending.unwrap(), point_limit())
                .unwrap()
                .unwrap(),
            before
        );
        let state = fixture
            .storage
            .turn_state(live.home(), pending.unwrap(), point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(state.lifecycle(), syndic_storage::TurnLifecycle::Pending);
        assert_eq!(state.source_event_count(), 0);
        assert!(live.home().pending_reconciliations().is_empty());
        drop(live);
        fence.try_reopen(true).unwrap();
    });
    fixture.close();
}
