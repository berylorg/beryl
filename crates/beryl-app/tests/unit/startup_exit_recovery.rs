use beryl_home_store::{
    HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};

fn published_home(directory: &tempfile::TempDir, faults: FaultController) -> HomeStore {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let _state = beryl_state::BerylState::register(&mut candidate).unwrap();
    candidate
        .prepare_publication(beryl_state::BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap()
}

#[test]
fn recovered_home_binding_preserves_request_and_fences_without_replaying_exit() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let home = published_home(&directory, faults.clone());
    let other_directory = tempfile::tempdir().unwrap();
    let other = published_home(&other_directory, FaultController::new());
    let producer = producer();
    let mut running = handoff(&producer);
    let window = WindowId::from_bytes([41; 16]);
    let retained = running.window_command(window);
    retained.request_exit();
    let request = take(&mut running);
    let foreign = request.test_foreign();
    let observer = Arc::new(ObserveWake(std::sync::atomic::AtomicUsize::new(0)));
    let wake = Waker::from(observer.clone());
    assert!(
        running
            .poll_exit(&mut Context::from_waker(&wake))
            .is_pending()
    );
    let refused = |attempt, home: &HomeStore| {
        let before = {
            let state = producer.0.borrow();
            (
                state
                    .exit_gates
                    .home
                    .as_ref()
                    .map(|(home, generation)| (home.home_id(), *generation)),
                state.exit_gates.home_unavailable,
            )
        };
        assert!(
            running
                .bind_recovered_home(attempt, home.service_reference())
                .is_err()
        );
        let state = producer.0.borrow();
        assert_eq!(
            before,
            (
                state
                    .exit_gates
                    .home
                    .as_ref()
                    .map(|(home, generation)| (home.home_id(), *generation)),
                state.exit_gates.home_unavailable
            )
        );
        assert!(state.wake.as_ref().unwrap().will_wake(&wake));
        assert!(running.is_active(&request));
    };
    refused(&request, &home);
    running.bind_home(home.service_reference());
    refused(&request, &home);
    refused(&foreign, &home);
    refused(&request, &other);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    refused(&request, &home);
    let home = home.recover_same_home().unwrap().publish().unwrap();
    assert_eq!(home.health().state(), HomeHealthState::Healthy);
    refused(&foreign, &home);
    running.set_gate(RunningExitGate::Unavailable, true);
    running.set_gate(RunningExitGate::SettingsReconciliation, true);
    running
        .bind_recovered_home(&request, home.service_reference())
        .unwrap();
    refused(&request, &home);
    refused(&request, &other);
    assert!(running.is_active(&request));
    assert_eq!(request.invoking_window(), Some(window));
    assert!(
        retained
            .disabled_reason()
            .unwrap()
            .contains("Beryl-home failure notice")
    );
    producer.request_exit();
    retained.request_exit();
    assert!(!producer.0.borrow().exit);
    assert!(producer.0.borrow().exit_window.is_none());
    assert_eq!(observer.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(producer.0.borrow().wake.as_ref().unwrap().will_wake(&wake));
    let completion_wake = running.finish_exit_deferred_wake(&request).unwrap();
    assert!(completion_wake.unwrap().will_wake(&wake));
    assert!(!running.exit_requested());
    assert!(
        running
            .bind_recovered_home(&request, home.service_reference())
            .is_err()
    );
    retained.request_exit();
    assert!(!running.exit_requested());
    running.set_gate(RunningExitGate::HomeUnavailable, false);
    assert!(
        retained
            .disabled_reason()
            .unwrap()
            .contains("Settings reconciliation")
    );
    running.set_gate(RunningExitGate::SettingsReconciliation, false);
    assert_eq!(
        retained.disabled_reason(),
        Some("Application Exit is not available.")
    );
    running.set_gate(RunningExitGate::Unavailable, false);
    assert_eq!(retained.disabled_reason(), None);
    assert!(!running.exit_requested());
    retained.request_exit();
    let fresh = take(&mut running);
    assert_eq!(fresh.invoking_window(), Some(window));
    assert!(!running.finish_exit(&request));
    assert!(running.finish_exit(&fresh));
    home.close().unwrap();
    assert!(retained.disabled_reason().is_some());
    other.close().unwrap();
}
