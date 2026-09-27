use super::recovery_support::{fail, installed};
use super::*;
use crate::app_services::recovery_retirement::ServiceGraphRetirementError;
use beryl_model::WindowId;

#[test]
fn recovery_retirement_preserves_home_registry_and_resident_occupancy() {
    let (directory, mut owner, faults) = installed();
    let reservation = owner
        .windows
        .reserve_main_window(WindowId::from_bytes([152; 16]))
        .unwrap();
    let graph = owner.graph().unwrap();
    let expected = graph.home().health().generation().unwrap();
    let restore = graph.restored_window_attempt().unwrap();
    let themes = graph.state().themes();
    owner.process.fence().unwrap();
    super::recovery_support::install_uncertain_enrollment(
        &owner,
        graph.home(),
        graph.syndic(),
        &faults,
    );
    let original = graph.home().pending_reconciliations().pop().unwrap();
    fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    assert!(owner.graph().is_none());
    assert_eq!(themes.diagnostics().active_subscriptions(), 0);
    assert!(restore.validate_lifetime().is_err());
    assert_eq!(owner.windows.main_window_occupancy(), 1);
    assert_eq!(owner.enrollments.pending_count(), 1);
    assert!(owner.process.execution_permit().commit(|| ()).is_err());
    assert!(!owner.initial_attempt_is_settled());
    assert!(matches!(
        owner.retire_failed_service_graph(expected),
        Err(ServiceGraphRetirementError::AlreadyRetained)
    ));
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    drop(reservation);
    let home = owner.test_retired_service_home().unwrap();
    assert_eq!(home.pending_reconciliations().len(), 1);
    let mut candidate = home.recover_same_home().unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert!(matches!(
        candidate
            .recovery_access()
            .unwrap()
            .reconcile(&original)
            .unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    owner
        .enrollments
        .settle_retired_candidate(
            &candidate.recovery_access().unwrap(),
            &syndic,
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(owner.enrollments.pending_count(), 0);
    candidate.abort().close().unwrap();
}

#[test]
fn recovery_retirement_refuses_healthy_and_stale_generation_without_consumption() {
    let (_directory, mut owner, faults) = installed();
    let expected = owner.graph().unwrap().home().health().generation().unwrap();
    assert!(matches!(
        owner.retire_failed_service_graph(expected),
        Err(ServiceGraphRetirementError::Stale)
    ));
    owner.process.execution_permit().commit(|| ()).unwrap();
    let (_other_directory, candidate, _, _, other_faults) = fixture();
    let home = candidate.publish().unwrap();
    other_faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let recovered = home.recover_same_home().unwrap();
    let later = recovered.generation();
    assert_ne!(expected, later);
    fail(&owner, &faults);
    assert!(matches!(
        owner.retire_failed_service_graph(later),
        Err(ServiceGraphRetirementError::Stale)
    ));
    assert!(owner.graph().is_some());
    owner.retire_failed_service_graph(expected).unwrap();
    owner.test_retired_service_home().unwrap().close().unwrap();
    recovered.abort().close().unwrap();
}

#[test]
fn incomplete_recovery_retirement_retains_lock_without_reopening_authority() {
    let (directory, mut owner, faults) = installed();
    let expected = owner.graph().unwrap().home().health().generation().unwrap();
    fail(&owner, &faults);
    owner.test_fail_shutdown_completion();
    assert!(matches!(
        owner.retire_failed_service_graph(expected),
        Err(ServiceGraphRetirementError::Incomplete)
    ));
    assert!(owner.graph().is_none());
    assert!(owner.test_retired_service_home().is_none());
    assert!(!owner.initial_attempt_is_settled());
    assert!(owner.process.execution_permit().commit(|| ()).is_err());
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    assert!(matches!(
        owner.retire_failed_service_graph(expected),
        Err(ServiceGraphRetirementError::AlreadyRetained)
    ));
    drop(owner);
    assert_reopens(&directory);
}
