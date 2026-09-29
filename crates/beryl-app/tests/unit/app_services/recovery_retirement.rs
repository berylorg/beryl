use super::recovery_support::{fail, installed};
use super::*;
use crate::app_services::recovery_retirement::{
    RetiredHomeRecoveryError, RetiredProcessWorkError, ServiceGraphRetirementError,
};
use beryl_model::WindowId;

#[test]
fn retired_home_construction_preserves_failed_custody_and_returns_private_candidate() {
    for fault in [
        None,
        Some(FaultPoint::BeforeReopen),
        Some(FaultPoint::AfterReopen),
    ] {
        let (directory, mut owner, faults) = installed();
        // Join the fixture scanner before arming a process-wide read fault.
        owner
            .graph
            .as_mut()
            .unwrap()
            .handoff
            .as_mut()
            .unwrap()
            .shutdown()
            .unwrap();
        let reservation = owner
            .windows
            .reserve_main_window(WindowId::from_bytes([153; 16]))
            .unwrap();
        let graph = owner.graph().unwrap();
        let expected = graph.home().health().generation().unwrap();
        let home_id = graph.home().home_id();
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
        if let Some(fault) = fault {
            faults.fail_next(fault);
            let error = owner.recover_retired_service_home(expected).unwrap_err();
            match fault {
                FaultPoint::BeforeReopen => assert!(matches!(
                    error,
                    RetiredHomeRecoveryError::Reopen(
                        beryl_home_store::HomeRecoveryError::Layout { .. }
                    )
                )),
                FaultPoint::AfterReopen => assert!(matches!(
                    error,
                    RetiredHomeRecoveryError::Reopen(
                        beryl_home_store::HomeRecoveryError::Persistence { .. }
                    )
                )),
                _ => unreachable!(),
            }
            assert!(
                HomeOpenCandidate::open(HomeOpenOptions::new(
                    directory.path(),
                    HomeSchemaVersion::CURRENT,
                ))
                .is_err()
            );
            assert!(owner.graph().is_none());
            assert_eq!(owner.enrollments.pending_count(), 1);
            assert_eq!(owner.windows.main_window_occupancy(), 1);
            assert!(owner.process.execution_permit().commit(|| ()).is_err());
        }
        let mut candidate = owner.recover_retired_service_home(expected).unwrap();
        let aborted_generation = candidate.generation();
        let mut returned = Some(candidate.abort());
        assert!(matches!(
            owner.return_retired_service_home(aborted_generation, &mut returned),
            Err(ServiceGraphRetirementError::Stale)
        ));
        assert!(returned.is_some());
        owner
            .return_retired_service_home(expected, &mut returned)
            .unwrap();
        assert!(returned.is_none());
        assert!(matches!(
            owner.return_retired_service_home(expected, &mut returned),
            Err(ServiceGraphRetirementError::InvalidHomeReturn)
        ));
        assert!(
            HomeOpenCandidate::open(HomeOpenOptions::new(
                directory.path(),
                HomeSchemaVersion::CURRENT,
            ))
            .is_err()
        );
        assert_eq!(owner.enrollments.pending_count(), 1);
        assert_eq!(owner.windows.main_window_occupancy(), 1);
        assert!(owner.process.execution_permit().commit(|| ()).is_err());
        candidate = owner.recover_retired_service_home(expected).unwrap();
        assert_ne!(candidate.generation(), aborted_generation);
        assert_eq!(candidate.home_id(), home_id);
        assert_ne!(candidate.generation(), expected);
        assert_eq!(
            candidate.service_reference().health().state(),
            HomeHealthState::Reopening
        );
        assert!(matches!(
            owner.recover_retired_service_home(expected),
            Err(RetiredHomeRecoveryError::Retirement(
                ServiceGraphRetirementError::HomeTransferred
            ))
        ));
        assert!(
            HomeOpenCandidate::open(HomeOpenOptions::new(
                directory.path(),
                HomeSchemaVersion::CURRENT,
            ))
            .is_err()
        );
        assert!(matches!(
            candidate
                .recovery_access()
                .unwrap()
                .reconcile(&original)
                .unwrap(),
            beryl_home_store::ReconciliationResolution::ExactNew { .. }
        ));
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        owner
            .settle_retired_process_work(
                &candidate.recovery_access().unwrap(),
                &state,
                &syndic,
                &CommandCancellation::new(),
            )
            .unwrap();
        assert_eq!(owner.enrollments.pending_count(), 0);
        assert_eq!(owner.windows.main_window_occupancy(), 1);
        assert!(owner.graph().is_none());
        assert!(owner.process.execution_permit().commit(|| ()).is_err());
        drop(reservation);
        candidate.abort().close().unwrap();
    }
}

#[test]
fn recovery_retirement_preserves_home_registry_and_resident_occupancy() {
    let (directory, mut owner, faults) = installed();
    // Join the fixture scanner before arming a process-wide read fault.
    owner
        .graph
        .as_mut()
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .shutdown()
        .unwrap();
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
    let home = owner.take_retired_service_home(expected).unwrap();
    assert!(matches!(
        owner.take_retired_service_home(expected),
        Err(ServiceGraphRetirementError::HomeTransferred)
    ));
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .is_err()
    );
    assert_eq!(home.pending_reconciliations().len(), 1);
    let mut candidate = home.recover_same_home().unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        owner.settle_retired_process_work(
            &candidate.recovery_access().unwrap(),
            &state,
            &syndic,
            &cancelled,
        ),
        Err(RetiredProcessWorkError::Cancelled)
    ));
    assert_eq!(owner.enrollments.pending_count(), 1);
    assert!(matches!(
        candidate
            .recovery_access()
            .unwrap()
            .reconcile(&original)
            .unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    for _ in 0..2 {
        owner
            .settle_retired_process_work(
                &candidate.recovery_access().unwrap(),
                &state,
                &syndic,
                &CommandCancellation::new(),
            )
            .unwrap();
    }
    assert_eq!(owner.enrollments.pending_count(), 0);
    assert_eq!(owner.settlements.pending_nondispatch_count(), 0);
    assert_eq!(owner.windows.main_window_occupancy(), 1);
    assert!(owner.graph().is_none());
    assert!(owner.process.execution_permit().commit(|| ()).is_err());
    drop(reservation);
    candidate.abort().close().unwrap();
}

#[test]
fn retired_process_work_refuses_absent_incomplete_and_foreign_retirement() {
    for incomplete in [false, true] {
        let (_directory, mut owner, faults) = installed();
        let expected = owner.graph().unwrap().home().health().generation().unwrap();
        let (_other_directory, candidate, _, _, other_faults) = fixture();
        let home = candidate.publish().unwrap();
        other_faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(home.home_revision().is_err());
        let mut candidate = home.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        assert!(matches!(
            owner.settle_retired_process_work(
                &access,
                &state,
                &syndic,
                &CommandCancellation::new()
            ),
            Err(RetiredProcessWorkError::RetirementIncomplete)
        ));
        fail(&owner, &faults);
        if incomplete {
            owner.test_fail_shutdown_completion();
            assert!(owner.retire_failed_service_graph(expected).is_err());
        } else {
            owner.retire_failed_service_graph(expected).unwrap();
        }
        let result = owner.settle_retired_process_work(
            &access,
            &state,
            &syndic,
            &CommandCancellation::new(),
        );
        if incomplete {
            assert!(matches!(
                result,
                Err(RetiredProcessWorkError::RetirementIncomplete)
            ));
        } else {
            assert!(matches!(
                result,
                Err(RetiredProcessWorkError::StaleCandidate)
            ));
            owner
                .take_retired_service_home(expected)
                .unwrap()
                .close()
                .unwrap();
        }
        assert!(owner.process.execution_permit().commit(|| ()).is_err());
        candidate.abort().close().unwrap();
    }
}

#[test]
fn recovery_retirement_refuses_healthy_and_stale_generation_without_consumption() {
    let (_directory, mut owner, faults) = installed();
    let expected = owner.graph().unwrap().home().health().generation().unwrap();
    assert!(matches!(
        owner.return_retired_service_home(expected, &mut None),
        Err(ServiceGraphRetirementError::Stale)
    ));
    assert!(matches!(
        owner.take_retired_service_home(expected),
        Err(ServiceGraphRetirementError::Stale)
    ));
    assert!(matches!(
        owner.recover_retired_service_home(expected),
        Err(RetiredHomeRecoveryError::Retirement(
            ServiceGraphRetirementError::Stale
        ))
    ));
    assert!(matches!(
        owner.retire_failed_service_graph(expected),
        Err(ServiceGraphRetirementError::Stale)
    ));
    owner.process.execution_permit().commit(|| ()).unwrap();
    assert!(matches!(
        owner.finish_service_graph_retirement(expected),
        Err(ServiceGraphRetirementError::Stale)
    ));
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
    assert!(matches!(
        owner.take_retired_service_home(later),
        Err(ServiceGraphRetirementError::Stale)
    ));
    assert!(matches!(
        owner.recover_retired_service_home(later),
        Err(RetiredHomeRecoveryError::Retirement(
            ServiceGraphRetirementError::Stale
        ))
    ));
    assert!(matches!(
        owner.finish_service_graph_retirement(later),
        Err(ServiceGraphRetirementError::Stale)
    ));
    owner.finish_service_graph_retirement(expected).unwrap();
    let mut foreign = Some(recovered.abort());
    assert!(matches!(
        owner.return_retired_service_home(expected, &mut foreign),
        Err(ServiceGraphRetirementError::InvalidHomeReturn)
    ));
    assert!(foreign.is_some());
    owner
        .take_retired_service_home(expected)
        .unwrap()
        .close()
        .unwrap();
    assert!(matches!(
        owner.return_retired_service_home(expected, &mut foreign),
        Err(ServiceGraphRetirementError::InvalidHomeReturn)
    ));
    foreign.take().unwrap().close().unwrap();
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
    assert!(matches!(
        owner.return_retired_service_home(expected, &mut None),
        Err(ServiceGraphRetirementError::Incomplete)
    ));
    assert!(matches!(
        owner.take_retired_service_home(expected),
        Err(ServiceGraphRetirementError::Incomplete)
    ));
    assert!(matches!(
        owner.recover_retired_service_home(expected),
        Err(RetiredHomeRecoveryError::Retirement(
            ServiceGraphRetirementError::Incomplete
        ))
    ));
    assert!(matches!(
        owner.finish_service_graph_retirement(expected),
        Err(ServiceGraphRetirementError::Incomplete)
    ));
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

#[test]
fn recovery_retirement_waits_for_admitted_marker_drive_without_reopening_admission() {
    let (directory, mut owner, faults) = installed();
    let flight = super::recovery_support::marker_flight(&owner);
    let graph = owner.graph().unwrap();
    let expected = graph.home().health().generation().unwrap();
    let marker = graph.marker();
    let home = graph.home().service_reference();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    marker.test_arm_before_command_fault(move |_| {
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(30)).unwrap();
    });
    let worker_marker = marker.clone();
    let worker = std::thread::spawn(move || worker_marker.drive(&home, flight));
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    fail(&owner, &faults);
    assert!(matches!(
        owner.retire_failed_service_graph(expected),
        Err(ServiceGraphRetirementError::MarkerDrivesPending)
    ));
    assert!(owner.graph().is_none());
    assert!(matches!(
        owner.take_retired_service_home(expected),
        Err(ServiceGraphRetirementError::Incomplete)
    ));
    assert!(matches!(
        owner.finish_service_graph_retirement(expected),
        Err(ServiceGraphRetirementError::MarkerDrivesPending)
    ));
    assert!(owner.process.execution_permit().commit(|| ()).is_err());
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .is_err()
    );
    release_tx.send(()).unwrap();
    assert!(worker.join().unwrap().is_err());
    assert_eq!(marker.diagnostics().current_flights(), 0);
    owner.finish_service_graph_retirement(expected).unwrap();
    owner.finish_service_graph_retirement(expected).unwrap();
    assert!(owner.process.execution_permit().commit(|| ()).is_err());
    assert!(!owner.initial_attempt_is_settled());
    owner
        .take_retired_service_home(expected)
        .unwrap()
        .close()
        .unwrap();
}
