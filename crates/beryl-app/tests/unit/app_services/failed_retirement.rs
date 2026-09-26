use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand, ReconciliationResolution};
use beryl_model::WindowId;
use beryl_state::{
    ApplySettings, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};

fn installed() -> (tempfile::TempDir, ProcessServiceOwner, FaultController) {
    let (directory, candidate, state, syndic, faults) = fixture();
    let mut owner = owner(&candidate);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while owner
        .graph()
        .unwrap()
        .handoff
        .as_ref()
        .unwrap()
        .test_completed_passes()
        == 0
    {
        assert!(
            Instant::now() < deadline,
            "initial handoff scan did not settle"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    (directory, owner, faults)
}

fn fail(owner: &ProcessServiceOwner, faults: &FaultController) {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(owner.graph().unwrap().home().home_revision().is_err());
    assert_eq!(
        owner.graph().unwrap().home().health().state(),
        HomeHealthState::Failed
    );
}

fn reopen(
    owner: &mut ProcessServiceOwner,
    directory: &tempfile::TempDir,
) -> Result<(), AppServiceOpenFailure> {
    let (candidate, state, syndic) = super::reopening::candidate_at(directory);
    owner.open_initial(
        candidate,
        state,
        syndic,
        configuration(),
        SyndicTimestamp::from_unix_millis(2),
        CommandCancellation::new(),
    )
}

#[test]
fn healthy_startup_retirement_rejects_without_fencing_the_graph() {
    let (_directory, mut owner, _) = installed();
    let original = owner.graph().unwrap().cas().service_generation();
    assert!(matches!(
        owner.retire_failed_startup(),
        Err(AppServiceCloseError::NotFailed)
    ));
    assert_eq!(owner.graph().unwrap().cas().service_generation(), original);
    owner.process.execution_permit().commit(|| ()).unwrap();
    close(&mut owner);
}

#[test]
fn failed_startup_retires_complete_graph_before_fresh_reopen() {
    let (directory, mut owner, faults) = installed();
    let graph = owner.graph().unwrap();
    let reference = graph.home().service_reference();
    let restore = graph.restored_window_attempt().unwrap();
    let themes = graph.state().themes();
    let old_permit = owner.process.execution_permit();
    fail(&owner, &faults);
    assert!(owner.begin_shutdown().is_err());
    owner.retire_failed_startup().unwrap();
    assert!(owner.graph().is_none());
    assert!(owner.retained_close().is_none());
    assert!(owner.failed_retirement.is_none());
    assert_eq!(themes.diagnostics().active_subscriptions(), 0);
    assert!(reference.home_revision().is_err());
    assert!(restore.validate_lifetime().is_err());
    reopen(&mut owner, &directory).unwrap();
    assert!(old_permit.commit(|| ()).is_err());
    owner.process.execution_permit().commit(|| ()).unwrap();
    close(&mut owner);
}

#[test]
fn failed_startup_retirement_cannot_discard_a_window_reservation() {
    let (directory, mut owner, faults) = installed();
    let reservation = owner
        .windows
        .reserve_main_window(WindowId::from_bytes([151; 16]))
        .unwrap();
    fail(&owner, &faults);
    assert!(matches!(
        owner.retire_failed_startup(),
        Err(AppServiceCloseError::NotReady)
    ));
    assert!(owner.graph().is_some());
    assert_eq!(owner.windows.main_window_occupancy(), 1);
    drop(reservation);
    owner.retire_failed_startup().unwrap();
    assert_reopens(&directory);
}

#[test]
fn failure_after_consuming_failed_graph_never_authorizes_reopen() {
    let (directory, mut owner, faults) = installed();
    fail(&owner, &faults);
    owner.test_fail_shutdown_completion();
    assert!(matches!(
        owner.retire_failed_startup(),
        Err(AppServiceCloseError::PersistentFailure)
    ));
    assert!(owner.graph().is_none());
    assert!(owner.retained_close().is_none());
    let rejected = reopen(&mut owner, &directory).unwrap_err();
    assert!(matches!(
        rejected.error,
        AppServiceOpenError::AlreadyInstalled
    ));
    rejected.rejected_candidate.unwrap().close().unwrap();
}

#[test]
fn failed_home_close_retains_exact_reconciliation_and_blocks_reopen() {
    let (directory, mut owner, faults) = installed();
    let graph = owner.graph().unwrap();
    let home = graph.home();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            graph.state().settings().apply(
                graph.state().settings().revision(home).unwrap(),
                ApplySettings::new(vec![SettingUpdate::new(
                    SettingKey::DeveloperInstructions,
                    ExpectedSettingRevision::Absent,
                    SettingValue::developer_instructions("original intent").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = home.execute(command) else {
        panic!("expected original reconciliation");
    };
    let original = reconciliation.install_and_handle();
    fail(&owner, &faults);
    assert!(owner.retire_failed_startup().is_err());
    assert!(owner.graph().is_none());
    assert_eq!(
        owner
            .retained_close()
            .unwrap()
            .pending_reconciliation_scopes(),
        Some(1)
    );
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    let store = owner
        .failed_close
        .take()
        .unwrap()
        .into_open_store()
        .unwrap();
    let mut recovered = store.recover_same_home().unwrap();
    assert!(matches!(
        recovered
            .recovery_access()
            .unwrap()
            .reconcile(&original)
            .unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    recovered.abort().close().unwrap();
    let rejected = reopen(&mut owner, &directory).unwrap_err();
    assert!(matches!(
        rejected.error,
        AppServiceOpenError::AlreadyInstalled
    ));
    rejected.rejected_candidate.unwrap().close().unwrap();
}

#[test]
fn joined_handoff_accepts_only_the_exact_failed_generation_read_refusal() {
    use crate::app_services::failed_retirement::settled_handoff;
    use crate::discussion_settlement::coordinator::HandoffCoordinatorError;
    use beryl_home_store::ReadError;
    use beryl_state::DurableJobReadError;
    let (_directory, mut owner, faults) = installed();
    fail(&owner, &faults);
    let expected = owner.graph().unwrap().home().health().generation().unwrap();
    let ReadError::HealthGate(gate) = owner.graph().unwrap().home().home_revision().unwrap_err()
    else {
        panic!("expected failed read gate");
    };
    assert!(
        settled_handoff(
            Err(HandoffCoordinatorError::Read(ReadError::HealthGate(gate))),
            expected
        )
        .is_ok()
    );
    assert!(
        settled_handoff(
            Err(HandoffCoordinatorError::Page(DurableJobReadError::Read(
                ReadError::HealthGate(gate)
            ))),
            expected
        )
        .is_ok()
    );
    let (_other_directory, candidate, _, _, other_faults) = fixture();
    let other = candidate.publish().unwrap();
    other_faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(other.home_revision().is_err());
    let recovered = other.recover_same_home().unwrap();
    let later_generation = recovered.generation();
    assert_ne!(later_generation, expected);
    recovered.abort().close().unwrap();
    assert!(
        settled_handoff(
            Err(HandoffCoordinatorError::Read(ReadError::HealthGate(gate))),
            later_generation
        )
        .is_err()
    );
    for failure in [
        HandoffCoordinatorError::Panicked,
        HandoffCoordinatorError::RecoveryRequired,
        HandoffCoordinatorError::Read(ReadError::GenerationPoisoned),
    ] {
        assert!(settled_handoff(Err(failure), expected).is_err());
    }
    owner.retire_failed_startup().unwrap();
}
