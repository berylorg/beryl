#[test]
fn recovered_projection_lane_stages_once_while_startup_is_closed() {
    let startup = crate::cas_projection::service_startup::ServiceStartupGate::closed_gate();
    let signal = AcceptedInputSchedulerSignal::new();
    let lane = RecoveredProjectionLane::new(2, Arc::clone(&startup), signal.clone());

    let staged = match lane.stage(Vec::new()) {
        Ok(staged) => staged,
        Err(error) => {
            let (reason, _) = error.into_parts();
            panic!("first stage failed: {reason:?}");
        }
    };
    assert_eq!(staged, 0);
    assert_eq!(signal.diagnostics().recovered_projection_retained(), 0);
    let second = lane
        .stage(Vec::new())
        .err()
        .expect("the exact replacement lane stages only once");
    let (reason, returned) = second.into_parts();

    assert_eq!(reason, RecoveredProjectionLaneStageReason::AlreadyStaged);
    assert!(returned.is_empty());
}

#[test]
fn recovered_projection_lane_rejects_staging_after_startup_opens() {
    let startup = crate::cas_projection::service_startup::ServiceStartupGate::open_gate();
    let lane = RecoveredProjectionLane::new(2, startup, AcceptedInputSchedulerSignal::new());

    let error = lane
        .stage(Vec::new())
        .err()
        .expect("publication closes the recovered-projection staging boundary");
    let (reason, returned) = error.into_parts();

    assert_eq!(
        reason,
        RecoveredProjectionLaneStageReason::StartupGateNotClosed
    );
    assert!(returned.is_empty());
}

#[test]
fn recovered_projection_lane_drains_before_reporting_mutex_poison() {
    let startup = crate::cas_projection::service_startup::ServiceStartupGate::closed_gate();
    let signal = AcceptedInputSchedulerSignal::new();
    let lane = RecoveredProjectionLane::new(2, startup, signal.clone());
    assert!(lane.stage(Vec::new()).is_ok());
    lane.poison_for_test();

    let (entries, poisoned) = lane.take_all();

    assert!(entries.is_empty());
    assert!(poisoned);
    assert_eq!(signal.diagnostics().recovered_projection_retained(), 0);
}

#[test]
fn recovered_projection_lane_uses_only_direct_in_flight_execution() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/cas_projection/accepted_input_scheduler/recovered_projection.rs"
    ));

    assert!(source.contains("execute_ordinary_turn_in_flight"));
    assert!(!source.contains("obtain_projection_in_flight"));
    assert!(!source.contains("try_acquire_scheduled_ordinary_or_arm"));
}
