use super::*;
use crate::{
    cas_projection::{
        CasProjectionCoordinator, MinimumTurnCaptureReserve, ProcessScheduledExecutionProvider,
        ProjectionServiceConfig,
    },
    process_admission::ProcessAdmissionGate,
};
use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicItemId,
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
    fn new() -> Self {
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
        let turn = source.submitted_turn_id();
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

mod reconciliation {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/shutdown_reconciliation.rs"
    ));
}

#[test]
fn pending_settlement_waits_for_admission_flight_and_continuation_custody() {
    let fixture = Fixture::new();
    let reservation = fixture.gate.execution_permit().reserve().unwrap();
    let fence = fixture.gate.fence().unwrap();
    assert!(fixture.read(&fence).unwrap().is_none());
    drop(reservation);
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let flight = coordinator.begin_projection(fixture.thread).unwrap();
    assert!(fixture.read(&fence).unwrap().is_none());
    drop(flight);
    let continuation = fixture
        .service
        .stop_coordinator
        .compaction_custody
        .reserve_continuation(fixture.thread, fixture.turn)
        .unwrap();
    assert!(fixture.read(&fence).unwrap().is_none());
    drop(continuation);
    let settled = fixture.read(&fence).unwrap().unwrap();
    let ShutdownThreadDisposition::Pending(pending) = settled.disposition() else {
        panic!("pending work changed disposition");
    };
    assert_eq!(pending.turn_id(), fixture.turn);
    assert_eq!(pending.thread_id(), fixture.thread);
    settled
        .revalidate(
            &fixture.service,
            &fixture.sessions,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert!(matches!(
        coordinator.begin_projection(fixture.thread),
        Err(ProjectionCoordinatorError::ProjectionInFlight { .. })
    ));
    drop(settled);
    drop(coordinator.begin_projection(fixture.thread).unwrap());
}

#[test]
fn foreign_and_reopened_fences_cannot_authorize_settlement() {
    let fixture = Fixture::new();
    let foreign = ProcessAdmissionGate::new().fence().unwrap();
    assert!(matches!(
        fixture.read(&foreign),
        Err(ShutdownThreadSettlementError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    let fence = fixture.gate.fence().unwrap();
    let proof = fixture.read(&fence).unwrap().unwrap();
    fence.reopen_if(true).unwrap();
    assert!(matches!(
        proof.revalidate(
            &fixture.service,
            &fixture.sessions,
            &ProjectionCancellationToken::new()
        ),
        Err(ShutdownThreadSettlementError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    drop(proof);
    let fresh = fixture.gate.fence().unwrap();
    assert!(matches!(
        fixture.read(&fence),
        Err(ShutdownThreadSettlementError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    assert!(fixture.read(&fresh).unwrap().is_some());
}

#[test]
fn durable_corruption_never_revalidates_a_pending_settlement() {
    let fixture = Fixture::new();
    let fence = fixture.gate.fence().unwrap();
    let proof = fixture.read(&fence).unwrap().unwrap();
    let ShutdownThreadDisposition::Pending(pending) = proof.disposition() else {
        unreachable!()
    };
    let home = fixture.service.home.as_deref().unwrap();
    let mut changes = syndic_storage::test_faults::FixtureBatch::new();
    changes
        .delete(syndic_storage::test_faults::FixtureDelete::ContentManifest(
            pending.input().id(),
        ))
        .unwrap();
    execute(
        home,
        fixture
            .service
            .storage
            .fixture_contribution(fixture.service.storage.revision(home).unwrap(), changes),
    );
    assert!(
        proof
            .revalidate(
                &fixture.service,
                &fixture.sessions,
                &ProjectionCancellationToken::new()
            )
            .is_err()
    );
    drop(proof);
    assert!(matches!(
        fixture.read(&fence),
        Err(ShutdownThreadSettlementError::Durable(
            SyndicReadError::Invariant(_)
        ))
    ));
}
