use beryl_home_store::CommandOutcome;
use syndic_storage::{CompactionSettlement, SettleCompactionOperation};

use super::compaction_support::CompactionFixture;

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
    fixture.store.validate_registered_domains().unwrap();
    assert_eq!(fixture.operation(id).home_id(), fixture.store.home_id());
}
