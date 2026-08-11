use beryl_home_store::{CommandOutcome, CursorReadLimits};
use syndic_storage::{
    AcceptedRouteEffectiveState, AcceptedRouteRevision, AdmitStopOperation, NextTurnReason,
    SafelyReopenStopOperation, StopCause, StopCauseSet,
};

use super::stop_support::{active_stop_fixture, admit_current_draft_as_accepted};

fn scan_limits() -> CursorReadLimits {
    CursorReadLimits::new(32, 1_000_000).unwrap()
}

#[test]
fn input_before_and_after_stop_remains_ordered_next_turn_work() {
    let fixture = active_stop_fixture("phase65-stop-preserves-queued-input");
    admit_current_draft_as_accepted(&fixture, "ready before stop", 120, 5);
    let pre_stop_gate = fixture.gate();
    let admission = AdmitStopOperation::new(
        fixture.operation_id,
        fixture.target.clone(),
        pre_stop_gate.revision(),
        pre_stop_gate.selected_route().unwrap(),
        StopCauseSet::from(StopCause::SelectedOperationControl),
    );
    match fixture
        .store
        .execute_current(fixture.storage.current_admit_stop_operation(admission))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean queued-work stop admission, got {outcome:?}"),
    }
    let stopped = fixture.stop();
    let stopped_route = stopped.admission().successor_stopped_route();

    let first_page = fixture
        .storage
        .accepted_route_page(
            &fixture.store,
            fixture.thread,
            stopped_route.generation(),
            stopped_route.revision(),
            None,
        )
        .unwrap();
    assert_eq!(first_page.records().len(), 1);
    assert_eq!(
        first_page.records()[0].effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::Stop)
    );

    admit_current_draft_as_accepted(&fixture, "queued after stop", 121, 6);
    let stopped_descendant = fixture.gate();
    assert_eq!(stopped_descendant.live_steering_count(), 0);
    assert_eq!(stopped_descendant.live_next_turn_count(), 2);
    assert_eq!(stopped_descendant.selected_route(), Some(stopped_route));
    fixture.store.validate_registered_domains().unwrap();

    let sources = fixture
        .storage
        .accepted_next_source_page(
            &fixture.store,
            fixture.storage.revision(&fixture.store).unwrap(),
            None,
            scan_limits(),
        )
        .unwrap();
    assert_eq!(sources.records().len(), 2);
    assert_eq!(
        sources.records()[0].generation(),
        stopped_route.generation()
    );
    assert_eq!(
        sources.records()[1].generation(),
        stopped_descendant.route_generation_high_water().unwrap()
    );
    assert_eq!(
        AcceptedRouteRevision::FIRST.get(),
        1,
        "post-stop generation begins at its first revision"
    );

    let reopen = SafelyReopenStopOperation::new(
        fixture.operation_id,
        fixture.target.clone(),
        stopped_descendant.revision(),
        fixture.stop().revision(),
    );
    match fixture
        .store
        .execute_current(fixture.storage.current_safely_reopen_stop_operation(reopen))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean queued-work stop reopen, got {outcome:?}"),
    }
    let reopened = fixture.gate();
    assert_eq!(reopened.live_steering_count(), 0);
    assert_eq!(reopened.live_next_turn_count(), 2);
    assert!(reopened.selected_route().unwrap().generation() > sources.records()[1].generation());
    let retained_sources = fixture
        .storage
        .accepted_next_source_page(
            &fixture.store,
            fixture.storage.revision(&fixture.store).unwrap(),
            None,
            scan_limits(),
        )
        .unwrap();
    assert_eq!(retained_sources.records().len(), 2);
    fixture.store.validate_registered_domains().unwrap();
}
