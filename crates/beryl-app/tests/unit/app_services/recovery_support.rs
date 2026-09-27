use super::*;
use crate::{runtime_activity_enrollment::ActivityEnrollmentCommandOutcome, support};
use beryl_model::{RuntimeId, SyndicItemId};
use syndic_storage::{
    ActivityEnrollmentPreparation, ActivityEnrollmentRequest, ActivityQuerySource,
    SyndicPointReadLimit,
};

pub(super) fn installed() -> (tempfile::TempDir, ProcessServiceOwner, FaultController) {
    let (directory, candidate, state, syndic, faults) = fixture();
    let mut owner = owner(&candidate);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while owner
        .graph()
        .unwrap()
        .handoff
        .as_ref()
        .unwrap()
        .test_completed_passes()
        == 0
    {
        assert!(
            Instant::now() < deadline,
            "initial handoff scan did not settle"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    (directory, owner, faults)
}

pub(super) fn fail(owner: &ProcessServiceOwner, faults: &FaultController) {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(owner.graph().unwrap().home().home_revision().is_err());
    assert_eq!(
        owner.graph().unwrap().home().health().state(),
        HomeHealthState::Failed
    );
}
pub(super) fn install_uncertain_enrollment(
    owner: &ProcessServiceOwner,
    home: &HomeStore,
    syndic: &SyndicStorage,
    faults: &FaultController,
) -> RuntimeId {
    let thread = support::id(30);
    support::seed_populated(home, syndic.clone());
    support::converge_and_release_terminal_history(
        home,
        syndic.clone(),
        thread,
        support::populated::source_turn(),
    );
    let turn = support::exact_cas::submit_current_draft(
        home,
        syndic.clone(),
        thread,
        support::draft_id(220),
        SyndicItemId::from_bytes([221; 16]),
        "pending enrollment",
        support::timestamp(100),
    );
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let execution = syndic
        .thread_execution(home, thread, limit)
        .unwrap()
        .unwrap();
    let runtime = execution.execution().runtime_id();
    let head = syndic
        .activity_query_head(home, thread, limit)
        .unwrap()
        .unwrap();
    let ActivityEnrollmentPreparation::Prepared(prepared) = syndic
        .prepare_activity_enrollment(
            home,
            ActivityEnrollmentRequest::first(
                ActivityQuerySource::new(thread, turn),
                execution.execution().clone(),
                head.revision(),
            ),
        )
        .unwrap()
    else {
        panic!("first real Activity enrollment must need publication")
    };
    let reservation = owner.enrollments.reserve(home, prepared).unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        reservation.execute(),
        ActivityEnrollmentCommandOutcome::Pending { .. }
    ));
    assert_eq!(owner.enrollments.pending_count(), 1);
    assert_eq!(home.pending_reconciliations().len(), 1);
    runtime
}
