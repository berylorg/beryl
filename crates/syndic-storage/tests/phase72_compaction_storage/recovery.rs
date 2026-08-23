use beryl_home_store::CommandOutcome;
use syndic_storage::{CompactionRecoveryCase, CompactionSettlement, SettleCompactionOperation};

use super::compaction_support::{CompactionFixture, point_limit};

#[test]
fn recovery_distinguishes_unissued_and_possible_dispatch_without_recreating_a_claim() {
    let fixture = CompactionFixture::new("phase72-compaction-live-recovery", 60);
    let id = fixture.admit(70, 10);
    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, id, point_limit())
            .unwrap()
            .unwrap(),
        CompactionRecoveryCase::CancelBeforeDispatch(_)
    ));

    fixture.claim(id);
    let claimed = fixture.operation(id);
    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, id, point_limit())
            .unwrap()
            .unwrap(),
        CompactionRecoveryCase::PossibleDispatch(_)
    ));

    let fixture = fixture.reopen();
    assert_eq!(fixture.operation(id), claimed);
    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, id, point_limit())
            .unwrap()
            .unwrap(),
        CompactionRecoveryCase::PossibleDispatch(_)
    ));
}

#[test]
fn recovery_requires_completed_marker_then_successful_terminal_for_success_finalization() {
    let fixture = CompactionFixture::new("phase72-compaction-success-recovery", 65);
    let id = fixture.admit(75, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);

    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, id, point_limit())
            .unwrap()
            .unwrap(),
        CompactionRecoveryCase::FinalizeSuccess(_)
    ));
}

#[test]
fn consumed_successful_compaction_reopens_as_valid_exact_authority() {
    let fixture = CompactionFixture::new("phase72-compaction-success-reopen", 70);
    let id = fixture.admit(80, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    let operation = fixture.operation(id);
    match fixture
        .store
        .execute_current(fixture.storage.current_settle_compaction_operation(
            SettleCompactionOperation::new(
                id,
                operation.revision(),
                CompactionSettlement::ManualSuccess,
            ),
        )) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean recovered compaction settlement, got {outcome:?}"),
    }

    let fixture = fixture.reopen();
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(fixture.operation(id).home_id(), fixture.store.home_id());
    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, id, point_limit())
            .unwrap()
            .unwrap(),
        CompactionRecoveryCase::Settled(_)
    ));
}
