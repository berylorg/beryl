use super::*;
use beryl_model::WindowId;
use std::sync::Arc;

#[test]
fn retired_selection_releases_old_process_admission_and_preserves_exact_registry_exclusion() {
    let (_directory, mut owner, faults) = recovery_support::installed();
    let window = WindowId::from_bytes([193; 16]);
    let reservation = owner.windows.reserve_main_window(window).unwrap();
    let lease = Arc::new(owner.admit_thread_creation(&[window], window).unwrap());
    let graph = owner.graph().unwrap();
    let generation = graph.home().health().generation().unwrap();
    assert!(owner.windows.test_process_closing_is_blocked());
    let alias = lease.clone();
    recovery_support::fail(&owner, &faults);
    let (lease, _) = lease
        .retire_failed_home(graph.home(), generation, window)
        .err()
        .unwrap();
    assert!(owner.windows.selection_pending());
    assert!(owner.windows.test_process_closing_is_blocked());
    drop(alias);
    let retired = lease
        .retire_failed_home(graph.home(), generation, window)
        .ok()
        .unwrap();
    assert!(owner.windows.selection_pending());
    assert!(owner.windows.test_close_is_blocked(&[window]));
    assert!(
        owner
            .windows
            .test_acquisition_is_blocked(WindowId::from_bytes([194; 16]))
    );
    assert!(!owner.windows.test_process_closing_is_blocked());
    retired.validate_owner().unwrap();
    owner.process.fence().unwrap();
    owner.retire_failed_service_graph(generation).unwrap();
    let mut candidate = owner.recover_retired_service_home(generation).unwrap();
    retired
        .validate_candidate(&candidate.recovery_access().unwrap())
        .unwrap();
    assert!(owner.windows.selection_pending());
    retired.release().ok().unwrap();
    assert!(!owner.windows.selection_pending());
    assert_eq!(owner.windows.main_window_occupancy(), 1);
    drop(candidate);
    drop(reservation);
}

#[test]
fn failed_selection_transfer_refuses_wrong_window_and_retained_membership_change() {
    let (_directory, owner, faults) = recovery_support::installed();
    let window = WindowId::from_bytes([195; 16]);
    let reservation = owner.windows.reserve_main_window(window).unwrap();
    let lease = Arc::new(owner.admit_thread_creation(&[window], window).unwrap());
    let graph = owner.graph().unwrap();
    let generation = graph.home().health().generation().unwrap();
    recovery_support::fail(&owner, &faults);
    let (lease, _) = lease
        .retire_failed_home(graph.home(), generation, WindowId::from_bytes([196; 16]))
        .err()
        .unwrap();
    assert!(owner.windows.selection_pending());
    let retired = lease
        .retire_failed_home(graph.home(), generation, window)
        .ok()
        .unwrap();
    drop(reservation);
    assert!(retired.validate_owner().is_err());
    let (retired, _) = retired.release().err().unwrap();
    assert!(owner.windows.selection_pending());
    drop(retired);
}
