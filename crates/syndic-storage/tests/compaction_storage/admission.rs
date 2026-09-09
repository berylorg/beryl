use beryl_home_store::{CommandError, CommandOutcome};
use syndic_storage::{
    CompactionAdmissionRead, CompactionAttemptNonce, CompactionOperationNonce, SyndicMutationError,
};

use super::compaction_support::{CompactionFixture, loaded_generation, point_limit, timestamp};

#[test]
fn stale_candidate_cannot_admit_a_second_operation_after_gate_advances() {
    let fixture = CompactionFixture::new("compaction-stale-admission", 211);
    let CompactionAdmissionRead::Admissible(candidate) = fixture
        .storage
        .compaction_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("idle valid binding must provide a candidate")
    };
    let stale = candidate.admission(
        CompactionOperationNonce::from_bytes([212; 16]),
        CompactionAttemptNonce::from_bytes([213; 16]),
        loaded_generation(),
        timestamp(30),
    );
    let stale_id = stale.operation_id();
    let winner = fixture.admit(214, 20);
    let before_gate = fixture.gate();
    let before_home = fixture.store.home_revision().unwrap();
    let outcome = fixture
        .store
        .execute_current(fixture.storage.current_admit_compaction_operation(stale));
    let CommandOutcome::NotCommitted {
        evidence: CommandError::ContributorValidation { source, .. },
    } = outcome
    else {
        panic!("stale admission must be proven not committed: {outcome:?}")
    };
    assert!(
        matches!(source.downcast_ref::<SyndicMutationError>(), Some(SyndicMutationError::InputGateRevisionConflict { expected, current })
        if *expected == candidate.source_gate_revision() && *current == before_gate.revision())
    );
    assert_eq!(fixture.store.home_revision().unwrap(), before_home);
    assert_eq!(fixture.gate(), before_gate);
    assert!(
        fixture
            .storage
            .compaction_operation(&fixture.store, stale_id, point_limit())
            .unwrap()
            .is_none()
    );
    let CompactionAdmissionRead::Existing(operation) = fixture
        .storage
        .compaction_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("the admitted winner must remain the only operation")
    };
    assert_eq!(operation.id(), winner);
}
