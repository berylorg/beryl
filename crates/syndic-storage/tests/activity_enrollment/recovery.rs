use super::*;

#[test]
fn candidate_reconciliation_preserves_exact_old_and_committed_enrollment() {
    for point in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let f = Fixture::new();
        let (command, witness) = f.prepare(None).into_command();
        f.faults.fail_next(point);
        match f.store.execute(command) {
            CommandOutcome::NotCommitted { .. } if point == FaultPoint::BeforeCommit => {}
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                reconciliation.install();
            }
            _ => panic!("injected outcome"),
        }
        if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(f.store.home_revision().is_err());
        }
        let mut candidate = f.store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        assert!(
            f.storage
                .activity_enrollment_status_candidate(&access, &witness)
                .is_err()
        );
        let result = fresh
            .activity_enrollment_status_candidate(&access, &witness)
            .unwrap();
        assert!(matches!(
            (point, result),
            (
                FaultPoint::BeforeCommit,
                ActivityEnrollmentStatus::NotCommitted
            ) | (
                FaultPoint::AfterCommitBeforePersist,
                ActivityEnrollmentStatus::Committed { token: Some(_) }
            )
        ));
        for pending in access.pending_reconciliations() {
            access.reconcile(&pending).unwrap();
        }
        candidate.publish().unwrap().close().unwrap();
    }
}
