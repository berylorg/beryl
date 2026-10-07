use super::*;
use crate::cas_projection::service::recovery_retirement::{
    CasRetirementError, CasRetirementFailure,
};

#[test]
fn failed_cas_retirement_preserves_home_lock_and_reconciliation_for_fresh_reopen() {
    let (directory, faults, state, shutdowns, service) = service();
    let generation = service.service_generation();
    let old_generation = service.home_generation();
    let (server, session) = admitted_connection(&service, 102_180);
    let retirement = session.connection_retirement_handle_for_test();
    let outage = Arc::clone(&service.outage_inventory);
    let handle = fail_with_reconciliation(&service, &state, &faults);
    let retired = match service.retire_for_recovery(generation) {
        Ok(retired) => retired,
        Err(_) => panic!("exact failed service must retire"),
    };
    assert!(retirement.is_retired());
    assert!(retirement.is_detached());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(
        outage.snapshot().state,
        crate::cas_projection::test_faults::OutageCaptureState::Unavailable
    );
    let (evidence, home) = retired.into_parts();
    let home = home.unwrap();
    assert_eq!(evidence.home_generation(), old_generation);
    assert_eq!(home.pending_reconciliations().len(), 1);
    assert!(
        beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    let mut candidate = home.recover_same_home().unwrap();
    assert_ne!(candidate.generation(), old_generation);
    BerylState::reacquire_candidate(&candidate).unwrap();
    {
        let access = candidate.recovery_access().unwrap();
        assert!(matches!(
            access.reconcile(&handle).unwrap(),
            beryl_home_store::ReconciliationResolution::ExactNew { .. }
        ));
    }
    candidate.abort().close().unwrap();
    drop(session);
    drop(retirement);
    server.join();
}

fn fail_with_reconciliation(
    service: &ProjectionConnectionService,
    state: &BerylState,
    faults: &FaultController,
) -> beryl_home_store::ReconciliationHandle {
    let live = service.live_home_command().unwrap();
    let home = live.home();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions("retained failed command").unwrap(),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(state.settings().apply(
            state.settings().revision(home).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let beryl_home_store::CommandOutcome::Indeterminate { reconciliation, .. } =
        home.execute(command)
    else {
        panic!("failed persisted command must retain reconciliation");
    };
    let handle = reconciliation.install_and_handle();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    drop(live);
    handle
}

#[test]
fn healthy_or_stale_retirement_returns_the_untouched_service() {
    for (stale, failed) in [(false, false), (true, false), (true, true)] {
        let (_directory, faults, state, shutdowns, service) = service();
        let generation = if stale {
            ProjectionServiceGeneration::allocate().unwrap()
        } else {
            service.service_generation()
        };
        if failed {
            fail_home(&service, state, &faults);
        }
        let service = match service.retire_for_recovery(generation) {
            Err(CasRetirementFailure::Rejected(service)) => service,
            _ => panic!("retirement must reject without disposing the healthy service"),
        };
        assert_eq!(service.live_home_command().is_ok(), !failed);
        assert_eq!(shutdowns.load(Ordering::SeqCst), 0);
        service.close().unwrap();
    }
}

#[test]
fn failed_connection_join_retains_only_terminal_disposal_custody() {
    let (directory, faults, state, _, service) = service();
    let generation = service.service_generation();
    let (server, session) = admitted_connection(&service, 102_181);
    let connection = Arc::clone(session.connection());
    connection.fail_next_ingester_join_for_test();
    fail_home(&service, state, &faults);
    let failure = match service.retire_for_recovery(generation) {
        Err(CasRetirementFailure::Disposal(failure)) => failure,
        _ => panic!("failed join cannot issue recovery custody"),
    };
    assert!(matches!(
        failure.error(),
        CasRetirementError::Disposal(ProjectionConnectionServiceCloseError::ConnectionShutdown)
    ));
    assert!(connection.is_detached());
    assert!(
        beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    let original_error = format!("{:?}", failure.error());
    let terminal_failure = failure
        .close()
        .expect_err("failed connection join cannot prove terminal close");
    assert!(format!("{terminal_failure:?}").contains(&original_error));
    assert!(connection.is_detached());
    assert!(
        beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .is_err()
    );
    drop(session);
    drop(connection);
    server.join();
    drop(terminal_failure);
}

#[test]
fn reference_only_retirement_never_manufactures_home_ownership() {
    let (_directory, faults, state, _, mut service) = service();
    let generation = service.service_generation();
    let home = service.owned_home.take().unwrap();
    fail_home(&service, state, &faults);
    let retired = match service.retire_for_recovery(generation) {
        Ok(retired) => retired,
        Err(_) => panic!("reference-only failed service must retire"),
    };
    assert!(retired.into_parts().1.is_none());
    home.recover_same_home().unwrap().abort().close().unwrap();
}

#[test]
fn failed_retirement_close_keeps_pending_custody_terminal_only() {
    const CHILD: &str = "BERYL_RETIREMENT_CUSTODY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "cas_projection::service::persistent_failure_tests::recovery_retirement_tests::failed_retirement_close_keeps_pending_custody_terminal_only", "--nocapture"])
            .env(CHILD, "1")
            .env("TMP", directory.path())
            .env("TEMP", directory.path())
            .spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "terminal custody child failed: {status}");
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("terminal custody child timed out");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        directory.close().unwrap();
        return;
    }
    let (directory, faults, state, _, service) = service();
    let generation = service.service_generation();
    let (server, session) = admitted_connection(&service, 102_182);
    session.connection().fail_next_ingester_join_for_test();
    let _handle = fail_with_reconciliation(&service, &state, &faults);
    let failure = match service.retire_for_recovery(generation) {
        Err(CasRetirementFailure::Disposal(failure)) => failure,
        _ => panic!("unclean retirement must retain terminal-only custody"),
    };
    let terminal_close = failure.close().unwrap_err();
    drop(terminal_close);
    assert!(
        beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    drop(session);
    server.join();
}
