use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use beryl_home_store::{
    HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use syndic_storage::SyndicStorage;

use super::*;
use crate::cas_projection::{
    MinimumTurnCaptureReserve, ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
    ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionProvider,
    ScheduledOrdinaryExecutionUnavailable,
};

struct ProviderProbe(Arc<AtomicUsize>);

impl ScheduledOrdinaryExecutionProvider for ProviderProbe {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn wait_until(description: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        std::thread::yield_now();
    }
}

#[test]
fn persistent_failure_terminally_disposes_and_makes_the_service_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let provider_shutdowns = Arc::new(AtomicUsize::new(0));
    let supervisor = TerminalServiceSupervisor::start(
        home,
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(ProviderProbe(Arc::clone(&provider_shutdowns))),
    )
    .unwrap();

    let lease = supervisor.acquire().unwrap();
    let live = lease.live_home_command().unwrap();
    let home = live.home();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions("terminal supervisor failure").unwrap(),
    );
    let contribution = state.settings().apply(
        state.settings().revision(home).unwrap(),
        ApplySettings::new(vec![update]).unwrap(),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    faults.panic_next(FaultPoint::BeforeCommit);
    assert!(catch_unwind(AssertUnwindSafe(|| home.execute(command))).is_err());
    drop(live);
    drop(lease);

    wait_until("the supervisor to finish terminal disposal", || {
        let diagnostics = supervisor.diagnostics();
        diagnostics.terminal_failures() == 1 && diagnostics.terminal_settled()
    });
    assert!(matches!(
        supervisor.acquire(),
        Err(ServiceAvailability::Unavailable)
    ));
    assert!(matches!(
        supervisor.shutdown(),
        Err(TerminalServiceShutdownError::TerminalUnavailable)
    ));
    assert_eq!(provider_shutdowns.load(Ordering::SeqCst), 1);
}
