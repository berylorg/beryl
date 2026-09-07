use super::*;
use beryl_app::{
    LifecycleYieldOutcome,
    cas_projection::{StopCoordinationError, WindowCloseStopOutcome},
};

#[test]
fn close_before_yield_registration_fences_only_the_current_turn() {
    let mut source = syndic::Fixture::new(180);
    let submitted = source.submit_text(" current work");
    let other_thread = source.create_ordinary(181);
    let other = source.submit_text_on(other_thread, " independent work");
    let gate_before = source
        .storage
        .input_gate(&source.home(), source.thread, point_limit())
        .unwrap()
        .unwrap();
    let revision_before = source.storage.revision(&source.home()).unwrap();
    for _ in 0..2 {
        assert!(matches!(
            source
                .store
                .stop_selected_operation_for_window_close(source.thread)
                .unwrap(),
            WindowCloseStopOutcome::Ineligible(
                syndic_storage::StopAdmissionIneligibility::PendingTurn { .. }
            )
        ));
        assert!(
            !source
                .store
                .record_lifecycle_yield_outcome(
                    source.thread,
                    submitted.turn,
                    LifecycleYieldOutcome::PhaseContinue
                )
                .unwrap()
        );
    }
    assert_eq!(
        source.storage.revision(&source.home()).unwrap(),
        revision_before
    );
    assert_eq!(
        source
            .storage
            .input_gate(&source.home(), source.thread, point_limit())
            .unwrap()
            .unwrap(),
        gate_before
    );
    assert!(
        source
            .store
            .record_lifecycle_yield_outcome(
                other_thread,
                other.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    assert_eq!(
        source
            .store
            .take_terminal_lifecycle_yield_outcome(other_thread, other.turn)
            .unwrap(),
        Some(LifecycleYieldOutcome::PhaseContinue)
    );
    assert!(
        source
            .store
            .record_lifecycle_yield_outcome(
                source.thread,
                submitted.turn,
                LifecycleYieldOutcome::PlanComplete
            )
            .unwrap()
    );
    assert_eq!(
        source
            .store
            .take_terminal_lifecycle_yield_outcome(source.thread, submitted.turn)
            .unwrap(),
        Some(LifecycleYieldOutcome::PlanComplete)
    );
    source.complete_with_assistant(submitted, " done");
    source
        .store
        .cancel_selected_continuation_for_window_close(source.thread)
        .unwrap();
    assert!(
        !source
            .store
            .record_lifecycle_yield_outcome(
                source.thread,
                submitted.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    let later = source.submit_text(" later ordinary work");
    assert!(
        source
            .store
            .record_lifecycle_yield_outcome(
                source.thread,
                later.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    assert!(
        !source
            .store
            .record_lifecycle_yield_outcome(
                source.thread,
                submitted.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    assert_eq!(
        source
            .store
            .take_terminal_lifecycle_yield_outcome(source.thread, later.turn)
            .unwrap(),
        Some(LifecycleYieldOutcome::PhaseContinue)
    );
    let (directory, service) = source.into_service();
    service.close().unwrap();
    directory.close().unwrap();
}

#[test]
fn close_during_noninterruptible_compaction_cancels_the_original_yield() {
    let fixture = LifecycleFixture::new(182, 220);
    let gate_before = fixture.input_gate();
    let original_tail = fixture.committed_tail();
    assert!(matches!(
        fixture
            .service
            .stop_selected_operation_for_window_close(fixture.thread_id)
            .unwrap(),
        WindowCloseStopOutcome::Ineligible(
            syndic_storage::StopAdmissionIneligibility::Compacting { .. }
        )
    ));
    assert_eq!(fixture.input_gate(), gate_before);
    assert!(
        !fixture
            .service
            .record_lifecycle_yield_outcome(
                fixture.thread_id,
                fixture.yielding_turn_id,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    assert_manual_success_without_continuation(&fixture, original_tail);
    fixture.close();
}

#[test]
fn close_wins_racing_staged_compaction_settlement() {
    let fixture = LifecycleFixture::new(183, 222);
    let original_tail = fixture.committed_tail();
    fixture.publish_success_prefix();
    let pause = fixture.harness.pause_after_lifecycle_staging().unwrap();
    let terminal_harness = fixture.harness.clone();
    let operation_id = fixture.operation_id;
    let terminal = thread::spawn(move || {
        terminal_harness
            .publish_provider_event(
                operation_id,
                CompactionProviderEvent::Terminal(syndic_storage::TurnEndStatus::complete()),
                SyndicTimestamp::from_unix_millis(72_020),
            )
            .unwrap();
    });
    pause.wait_until_staged();
    fixture
        .service
        .cancel_selected_continuation_for_window_close(fixture.thread_id)
        .unwrap();
    fixture
        .service
        .cancel_selected_continuation_for_window_close(fixture.thread_id)
        .unwrap();
    pause.release();
    terminal.join().unwrap();
    assert_manual_success_without_continuation(&fixture, original_tail);
    fixture.close();
}

#[test]
fn close_preserves_a_continuation_that_already_won_durable_admission() {
    let fixture = LifecycleFixture::new(184, 224);
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    let operation = fixture.operation();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("compaction did not settle")
    };
    assert!(matches!(
        witness.settlement(),
        CompactionSettlement::LifecycleContinuation { .. }
    ));
    let gate_before = fixture.input_gate();
    assert!(matches!(
        gate_before.state(),
        syndic_storage::InputGateState::PendingTurn(_)
    ));
    let revision = fixture
        .storage
        .revision(fixture.service.live_home_command().unwrap().home())
        .unwrap();
    fixture
        .service
        .cancel_selected_continuation_for_window_close(fixture.thread_id)
        .unwrap();
    assert_eq!(fixture.input_gate(), gate_before);
    assert_eq!(fixture.operation(), operation);
    assert_eq!(
        fixture
            .storage
            .revision(fixture.service.live_home_command().unwrap().home())
            .unwrap(),
        revision
    );
    assert!(
        !fixture
            .service
            .record_lifecycle_yield_outcome(
                fixture.thread_id,
                fixture.yielding_turn_id,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    fixture.close();
}

#[test]
fn close_cancellation_preserves_accepted_user_work_through_settlement() {
    let fixture = LifecycleFixture::with_accepted_next(185, 226);
    let before = fixture.input_gate();
    let original_tail = fixture.committed_tail();
    fixture
        .service
        .cancel_selected_continuation_for_window_close(fixture.thread_id)
        .unwrap();
    assert_eq!(fixture.input_gate(), before);
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    assert_manual_success_without_continuation(&fixture, original_tail);
    let after = fixture.input_gate();
    assert_eq!(after.live_count(), before.live_count());
    assert_eq!(after.accepted_high_water(), before.accepted_high_water());
    assert_eq!(
        after.live_logical_utf8_bytes(),
        before.live_logical_utf8_bytes()
    );
    assert_eq!(after.selected_route(), before.selected_route());
    assert_eq!(after.live_next_turn_count(), 1);
    fixture.close();
}

#[test]
fn close_cancellation_rejects_retired_compaction_authority() {
    let fixture = LifecycleFixture::new(186, 228);
    fixture.harness.request_shutdown().unwrap();
    assert!(matches!(
        fixture
            .service
            .cancel_selected_continuation_for_window_close(fixture.thread_id),
        Err(StopCoordinationError::HomeAuthorityLost)
    ));
    fixture.close();
}

#[test]
fn close_after_yielding_terminal_consumes_intent_before_compaction() {
    let mut source = syndic::Fixture::new(187);
    let submitted = source.submit_text(" yielding work");
    assert!(
        source
            .store
            .record_lifecycle_yield_outcome(
                source.thread,
                submitted.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    source.complete_with_assistant(submitted, " finished");
    let revision = source.storage.revision(&source.home()).unwrap();
    assert!(matches!(
        source
            .store
            .stop_selected_operation_for_window_close(source.thread)
            .unwrap(),
        WindowCloseStopOutcome::Ineligible(syndic_storage::StopAdmissionIneligibility::Idle { .. })
    ));
    assert_eq!(
        source
            .store
            .take_terminal_lifecycle_yield_outcome(source.thread, submitted.turn)
            .unwrap(),
        None
    );
    assert!(
        !source
            .store
            .record_lifecycle_yield_outcome(
                source.thread,
                submitted.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    assert_eq!(source.storage.revision(&source.home()).unwrap(), revision);
    assert_eq!(
        source
            .store
            .context_compaction_diagnostics()
            .retained_operations(),
        0
    );
    let (directory, service) = source.into_service();
    let _ = service.close().unwrap();
    directory.close().unwrap();
}

#[test]
fn cancellation_capacity_preserves_live_fences_and_reclaims_terminal_work() {
    let mut source = syndic::Fixture::new(188);
    let first = source.submit_text(" first work");
    let second_thread = source.create_ordinary(189);
    let second = source.submit_text_on(second_thread, " second work");
    let third_thread = source.create_ordinary(190);
    let third = source.submit_text_on(third_thread, " third work");
    let harness = source
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    harness
        .cancel_window_close_continuation_with_capacity(source.thread, 2)
        .unwrap();
    harness
        .cancel_window_close_continuation_with_capacity(second_thread, 2)
        .unwrap();
    assert!(matches!(
        harness.cancel_window_close_continuation_with_capacity(third_thread, 2),
        Err(StopCoordinationError::ContinuationCancellationCapacity)
    ));
    assert!(
        source
            .store
            .record_lifecycle_yield_outcome(
                third_thread,
                third.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    for (thread_id, turn_id) in [(source.thread, first.turn), (second_thread, second.turn)] {
        harness
            .cancel_window_close_continuation_with_capacity(thread_id, 2)
            .unwrap();
        assert!(
            !source
                .store
                .record_lifecycle_yield_outcome(
                    thread_id,
                    turn_id,
                    LifecycleYieldOutcome::PhaseContinue
                )
                .unwrap()
        );
    }
    source.complete_with_assistant(first, " first finished");
    harness
        .cancel_window_close_continuation_with_capacity(third_thread, 2)
        .unwrap();
    assert_eq!(
        source
            .store
            .take_terminal_lifecycle_yield_outcome(third_thread, third.turn)
            .unwrap(),
        None
    );
    assert!(
        !source
            .store
            .record_lifecycle_yield_outcome(
                source.thread,
                first.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    assert!(
        !source
            .store
            .record_lifecycle_yield_outcome(
                second_thread,
                second.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    let later = source.submit_text(" later work");
    assert!(
        source
            .store
            .record_lifecycle_yield_outcome(
                source.thread,
                later.turn,
                LifecycleYieldOutcome::PhaseContinue
            )
            .unwrap()
    );
    let (directory, service) = source.into_service();
    let _ = service.close().unwrap();
    directory.close().unwrap();
}

fn assert_manual_success_without_continuation(
    fixture: &LifecycleFixture,
    original_tail: Option<beryl_model::SyndicTurnId>,
) {
    let operation = fixture.operation();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("compaction did not settle")
    };
    assert_eq!(witness.settlement(), &CompactionSettlement::ManualSuccess);
    assert_eq!(
        fixture.input_gate().state(),
        &syndic_storage::InputGateState::Idle
    );
    assert_eq!(fixture.committed_tail(), original_tail);
    assert_eq!(
        fixture
            .service
            .take_terminal_lifecycle_yield_outcome(fixture.thread_id, fixture.yielding_turn_id)
            .unwrap(),
        None
    );
    assert_eq!(
        fixture
            .service
            .context_compaction_diagnostics()
            .retained_operations(),
        0
    );
}
