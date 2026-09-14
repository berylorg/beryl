use crate::cas_projection::{
    ProjectionCancellationToken, ProjectionConnectionService, ScheduledExecutionSessions,
    service::{ShutdownThreadDisposition, ShutdownThreadSettlement, ShutdownThreadSettlementError},
};
use crate::{
    cas_projection::{
        CasProjectionCoordinator, MinimumTurnCaptureReserve, ProcessScheduledExecutionProvider,
        ProjectionServiceConfig,
    },
    process_admission::{ProcessAdmissionFence, ProcessAdmissionGate},
};
use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::{AssetState, BerylState};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
};

mod submission_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/submission_fixture.rs"
    ));
}

struct Fixture {
    service: ProjectionConnectionService,
    sessions: ScheduledExecutionSessions,
    gate: ProcessAdmissionGate,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    assets: AssetState,
    faults: beryl_home_store::test_faults::FaultController,
    _directory: tempfile::TempDir,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

impl Fixture {
    fn acquired_projection_flight(
        &self,
        thread: SyndicThreadId,
    ) -> crate::cas_projection::service::ProjectionFlight {
        let acquisition = crate::cas_projection::acquisition::ProjectionAcquisition::admit(
            &self.service.live_command_authorizer(),
        )
        .unwrap();
        CasProjectionCoordinator::for_healthy_home(self.service.live_home_command().unwrap().home())
            .unwrap()
            .begin_projection(thread)
            .unwrap()
            .with_acquisition(acquisition)
    }

    fn new() -> Self {
        Self::with_pending(true)
    }

    fn idle() -> Self {
        Self::with_pending(false)
    }

    fn with_pending(pending: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = beryl_home_store::test_faults::FaultController::new();
        let mut home = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let assets = BerylState::register(&mut home).unwrap().assets();
        let thread = SyndicThreadId::from_bytes([71; 16]);
        let binding = ExecutionBinding::new(
            RuntimeId::from_bytes([71; 16]),
            RootId::from_bytes([72; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\work\beryl",
            )
            .unwrap(),
        );
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([72; 16]),
                    binding,
                    SyndicTimestamp::from_unix_millis(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
        let turn = if pending {
            let (_, source) = submission_fixture::submit_atoms(
                &home,
                storage.clone(),
                assets.clone(),
                thread,
                SyndicDraftId::from_bytes([73; 16]),
                SyndicItemId::from_bytes([74; 16]),
                &[submission_fixture::Atom::Text(
                    "preserve pending at shutdown",
                )],
                75,
                SyndicTimestamp::from_unix_millis(3),
            );
            source.submitted_turn_id()
        } else {
            SyndicTurnId::from_bytes([75; 16])
        };
        let gate = ProcessAdmissionGate::new();
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let service = ProjectionConnectionService::new(
            gate.clone(),
            home,
            storage,
            ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
            Box::new(provider),
        )
        .unwrap();
        Self {
            service,
            sessions,
            gate,
            thread,
            turn,
            assets,
            faults,
            _directory: directory,
        }
    }

    fn read(
        &self,
        fence: &ProcessAdmissionFence,
    ) -> Result<Option<ShutdownThreadSettlement>, ShutdownThreadSettlementError> {
        self.service.try_shutdown_thread_settlement(
            &self.sessions,
            fence,
            self.thread,
            self.turn,
            &ProjectionCancellationToken::new(),
        )
    }
}
