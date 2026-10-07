use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeHealthState, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeServiceReference, ReconciliationResolution, test_faults::FaultPoint,
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use syndic_storage::SyndicStorage;

use super::*;
use crate::cas_projection::{
    MinimumTurnCaptureReserve, ProjectionConnectionServiceCloseOutcome, ProjectionServiceConfig,
    ScheduledExecutionProviderContext,
};

struct IdleProvider;

impl ScheduledOrdinaryExecutionProvider for IdleProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

struct AttachPanicProvider {
    reference: HomeServiceReference,
    shutdowns: Arc<AtomicUsize>,
    health_during_shutdown: Arc<Mutex<Option<HomeHealthState>>>,
}

impl ScheduledOrdinaryExecutionProvider for AttachPanicProvider {
    fn attach(&mut self, _: ScheduledExecutionProviderContext) {
        panic!("fixture attach panic");
    }

    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        *self.health_during_shutdown.lock().unwrap() = Some(self.reference.health().state());
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

fn opened_home() -> (
    tempfile::TempDir,
    beryl_home_store::HomeStore,
    SyndicStorage,
    BerylState,
    beryl_home_store::test_faults::FaultController,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = beryl_home_store::test_faults::FaultController::new();
    let mut home = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let home = home
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    (directory, home, storage, state, faults)
}

fn config() -> ProjectionServiceConfig {
    ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap()).unwrap()
}

#[test]
fn explicit_close_drains_unborrowed_command_permit_retires_reference_and_reopens() {
    let (directory, home, storage, _, _) = opened_home();
    let retained = home.service_reference();
    let service = ProjectionConnectionService::new(
        Default::default(),
        home,
        storage,
        config(),
        Box::new(IdleProvider),
    )
    .unwrap();
    let permit = service.command_authorizer.authorize().unwrap();
    let (closed, observed) = mpsc::sync_channel(1);
    let closer = thread::spawn(move || {
        closed.send(service.close()).unwrap();
    });
    assert!(observed.recv_timeout(Duration::from_secs(1)).is_err());
    drop(permit);
    assert!(matches!(
        observed
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    closer.join().unwrap();
    assert!(retained.home_revision().is_err());
    let reopened = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ));
    assert!(reopened.is_ok());
}

#[test]
fn attach_panic_shuts_provider_before_home_retirement_and_releases_retained_reference() {
    let (directory, home, storage, _, _) = opened_home();
    let retained = home.service_reference();
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let health_during_shutdown = Arc::new(Mutex::new(None));
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = ProjectionConnectionService::new(
            Default::default(),
            home,
            storage,
            config(),
            Box::new(AttachPanicProvider {
                reference: retained.clone(),
                shutdowns: Arc::clone(&shutdowns),
                health_during_shutdown: Arc::clone(&health_during_shutdown),
            }),
        );
    }));
    assert!(result.is_err());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(
        *health_during_shutdown.lock().unwrap(),
        Some(HomeHealthState::Healthy)
    );
    assert!(retained.home_revision().is_err());
    let reopened = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ));
    assert!(reopened.is_ok());
}

#[test]
fn pending_reconciliation_close_returns_owner_until_reconciliation_and_final_close() {
    let (directory, home, storage, state, faults) = opened_home();
    let retained = home.service_reference();
    let service = ProjectionConnectionService::new(
        Default::default(),
        home,
        storage,
        config(),
        Box::new(IdleProvider),
    )
    .unwrap();
    service.command_gate.close_for_shutdown();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions("retain close custody").unwrap(),
    );
    let contribution = state.settings().apply(
        state.settings().revision(&retained).unwrap(),
        ApplySettings::new(vec![update]).unwrap(),
    );
    let mut command = HomeCommand::new(retained.home_revision().unwrap());
    command.add(contribution).unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = retained.execute(command) else {
        panic!("faulted mutation must retain reconciliation custody");
    };
    let handle = reconciliation.install_and_handle();
    let mut failure = service.close().unwrap_err();
    for _ in 0..2 {
        let crate::cas_projection::ProjectionConnectionServiceCloseError::HomeClose(error) =
            failure.error()
        else {
            panic!("pending reconciliation must retain the original home close error");
        };
        assert_eq!(error.pending_reconciliation_scopes(), Some(1));
        failure = failure
            .retry()
            .expect_err("unsettled reconciliation cannot become successful close on retry");
        assert!(retained.home_revision().is_ok());
        assert!(
            HomeOpenCandidate::open(HomeOpenOptions::new(
                directory.path(),
                HomeSchemaVersion::CURRENT
            ))
            .is_err()
        );
    }
    let (error, mut service_custody) = failure.into_parts();
    assert!(!service_custody.settled);
    assert!(service_custody.shutdown_started);
    assert!(matches!(
        service_custody.close_inner(),
        Err(crate::cas_projection::ProjectionConnectionServiceCloseError::ShutdownIncomplete)
    ));
    let crate::cas_projection::ProjectionConnectionServiceCloseError::HomeClose(error) = error
    else {
        panic!("pending reconciliation must retain the service-owned home");
    };
    assert_eq!(error.pending_reconciliation_scopes(), Some(1));
    let owner = error
        .into_open_store()
        .expect("pending reconciliation close error retains the sole home owner");
    assert!(retained.home_revision().is_ok());
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .is_err()
    );
    assert!(matches!(
        owner.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    owner.close().unwrap();
    assert!(retained.home_revision().is_err());
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .is_ok()
    );
}
