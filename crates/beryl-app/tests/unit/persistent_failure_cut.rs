use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenError, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    BindingRevision, CasThreadId, PathFlavor, RootId, RuntimeMode, RuntimeNativePath,
    ThreadRevision,
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use syndic_storage::{
    CasLineageProof, CasRepresentedPrefixProof, NativeCasLineage, SyndicStorage,
    empty_selected_path_digest,
};

use super::*;
use crate::cas_projection::{
    LoadedCasProjection, LoadedProjectionReleaseOutcome, PersistentFailureNotificationStatus,
    PersistentFailureRecoveryInventoryError, connection::LoadedProjectionLease,
    persistent_failure::PersistentFailureServiceEscrowReservation,
};

mod admission_server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/phase37_normal_terminal/server.rs"
    ));
}

#[derive(Clone)]
struct ShutdownProbe {
    count: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for ShutdownProbe {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

fn service() -> (
    tempfile::TempDir,
    FaultController,
    BerylState,
    Arc<AtomicUsize>,
    ProjectionConnectionService,
) {
    service_with_worker_capacity(4)
}

fn service_with_worker_capacity(
    worker_capacity: u64,
) -> (
    tempfile::TempDir,
    FaultController,
    BerylState,
    Arc<AtomicUsize>,
    ProjectionConnectionService,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(8, worker_capacity).unwrap(),
        Box::new(ShutdownProbe {
            count: Arc::clone(&shutdowns),
        }),
    )
    .unwrap();
    wait_until(
        "the initial recovered-pending scheduler pass to settle",
        || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            diagnostics.recovered_pending_pass_count() >= 1
                && diagnostics.workers_active() == 0
                && service.worker_pool_diagnostics().active() == 0
                && !diagnostics.fatal()
        },
    );
    (directory, faults, state, shutdowns, service)
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

fn fail_home_through_live_command(
    service: &ProjectionConnectionService,
    state: BerylState,
    faults: &FaultController,
) {
    let (live, command) = prepare_failure_command(service, state, "persistent failure cut");
    let home = live.home();
    faults.panic_next(FaultPoint::BeforeCommit);

    let panicked = catch_unwind(AssertUnwindSafe(|| home.execute(command)));
    assert!(panicked.is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    assert!(matches!(
        service.persistent_failure_cut_snapshot().state(),
        PersistentFailureCutState::Armed | PersistentFailureCutState::Cutting
    ));
    drop(live);
}

fn prepare_failure_command<'a>(
    service: &'a ProjectionConnectionService,
    state: BerylState,
    instructions: &'static str,
) -> (LiveHomeCommand<'a>, HomeCommand) {
    let live = service.live_home_command().unwrap();
    let home = live.home();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions(instructions).unwrap(),
    );
    let contribution = state.settings().apply(
        state.settings().revision(home).unwrap(),
        ApplySettings::new(vec![update]).unwrap(),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    (live, command)
}


include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/persistent_failure_cut/cut_outcomes.rs"));
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/persistent_failure_cut/service_lifecycle.rs"));
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/persistent_failure_cut/retained_authority.rs"));
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/persistent_failure_cut/handoff_late.rs"));
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/persistent_failure_cut/barriers_and_incomplete.rs"));
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/pending_projection_quarantine.rs"));
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/pending_projection_quarantine_retirement.rs"));
