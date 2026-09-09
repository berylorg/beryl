use super::*;

#[test]
fn continuation_capacity_denial_preserves_acceptance_and_other_outcomes() {
    let fixture = StopFixture::new(62);
    let pressure: Vec<_> = (0..72)
        .map(|_| fixture.coordinator.compaction_custody.reserve().unwrap())
        .collect();
    assert!(
        !fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                fixture.turn,
                LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );
    assert!(
        fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .lifecycle_yields
            .is_empty()
    );
    assert_eq!(fixture.coordinator.compaction_custody.in_use(), 72);
    assert!(
        fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                fixture.turn,
                LifecycleYieldOutcome::PlanComplete,
            )
            .unwrap()
    );
    fixture
        .coordinator
        .release_ordinary_lifecycle_yield(fixture.thread, fixture.turn);
    drop(pressure);
    assert_eq!(fixture.coordinator.compaction_custody.in_use(), 0);
}

#[test]
fn cancelled_removed_continuation_and_compaction_share_one_slot_until_both_dispose() {
    let fixture = StopFixture::new(63);
    let pressure: Vec<_> = (0..71)
        .map(|_| fixture.coordinator.compaction_custody.reserve().unwrap())
        .collect();
    assert!(
        fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                fixture.turn,
                LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );
    assert!(
        !fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                fixture.turn,
                LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );
    let compaction_turn = SyndicTurnId::from_bytes([0xa5; 16]);
    let shared = fixture
        .coordinator
        .share_continuation_custody(fixture.thread, fixture.turn, compaction_turn)
        .unwrap();
    fixture
        .coordinator
        .bind_lifecycle_compaction(fixture.thread, fixture.turn, compaction_turn)
        .unwrap();
    assert!(
        fixture
            .coordinator
            .share_continuation_custody(
                fixture.thread,
                SyndicTurnId::from_bytes([0xa6; 16]),
                compaction_turn,
            )
            .is_err()
    );
    assert!(
        fixture
            .coordinator
            .share_continuation_custody(
                fixture.thread,
                fixture.turn,
                SyndicTurnId::from_bytes([0xa7; 16]),
            )
            .is_err()
    );
    fixture.coordinator.cancel_all_lifecycle_continuations();
    let accepted = fixture
        .coordinator
        .take_lifecycle_continuation(fixture.thread, fixture.turn)
        .unwrap();
    assert_eq!(accepted.effective_outcome(), None);
    assert!(
        fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .lifecycle_yields
            .is_empty()
    );
    assert!(fixture.coordinator.compaction_custody.reserve().is_none());
    drop(accepted);
    assert_eq!(fixture.coordinator.compaction_custody.in_use(), 72);
    drop(shared);
    assert_eq!(fixture.coordinator.compaction_custody.in_use(), 71);
    drop(pressure);
}

#[test]
fn continuation_disposal_after_shared_command_unwind_releases_last_slot() {
    let fixture = StopFixture::new(64);
    assert!(
        fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                fixture.turn,
                LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );
    let shared = fixture
        .coordinator
        .share_continuation_custody(
            fixture.thread,
            fixture.turn,
            SyndicTurnId::from_bytes([0xa8; 16]),
        )
        .unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _shared = shared;
            panic!("shared custody unwind");
        }))
        .is_err()
    );
    assert_eq!(fixture.coordinator.compaction_custody.in_use(), 1);
    fixture
        .coordinator
        .release_ordinary_lifecycle_yield(fixture.thread, fixture.turn);
    assert_eq!(fixture.coordinator.compaction_custody.in_use(), 0);
}
