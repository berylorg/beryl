use std::{path::Path, thread};

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions};
use beryl_model::{CasProcessGeneration, SyndicThreadId};
use syndic_storage::SyndicTimestamp;

use crate::{
    EXECUTION_ROOT, NoopBranch, NoopLifecycle, noop_handlers,
    server::{AUTHORIZATION, RawCasServer, TIMEOUT},
    syndic::{Fixture, execution_binding},
};

pub type ExecutionResult = Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>;

pub struct PreparedExecution {
    coordinator: CasProjectionCoordinator,
    session: AdmittedProjectionSession,
    projection: LoadedCasProjection,
}

pub struct CompletedExecution {
    pub result: ExecutionResult,
    pub session: AdmittedProjectionSession,
}

impl PreparedExecution {
    pub fn new(fixture: &Fixture, thread: SyndicThreadId, server: &RawCasServer) -> Self {
        let connector =
            ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
        let run_id = server.identity().run_id();
        let generation =
            CasProcessGeneration::new(380_000_u64.checked_add(run_id).unwrap()).unwrap();
        let mut session = fixture
            .store
            .admit_runtime_lifecycle_test_candidate(
                &connector,
                execution_binding(),
                generation,
                Path::new(EXECUTION_ROOT),
                TIMEOUT,
            )
            .unwrap();
        let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
        let request = CasProjectionRequest::new(
            thread,
            fixture.selected_path(thread),
            execution_binding(),
            ThreadStartOptions::persistent(),
            Some(2_000_000),
            SyndicTimestamp::from_unix_millis(380_000_u64.checked_add(run_id).unwrap()),
            TIMEOUT,
        );
        let projection = coordinator
            .obtain_projection(
                &*fixture.home(),
                &fixture.storage,
                &mut session,
                &request,
                &fixture.cancellation,
            )
            .unwrap();
        server.wait_for_projection();
        assert_eq!(
            projection.cas_thread_id().as_str(),
            server.identity().thread_id()
        );
        Self {
            coordinator,
            session,
            projection,
        }
    }

    pub fn install_target_abandonment(
        &self,
        thread: SyndicThreadId,
    ) -> beryl_app::cas_projection::test_faults::LiveEventTargetAbandonmentController {
        beryl_app::cas_projection::test_faults::install_live_event_target_abandonment(
            &self.session,
            thread,
        )
    }

    pub fn execute(
        self,
        fixture: &Fixture,
        request: &OrdinaryTurnExecutionRequest,
        while_running: impl FnOnce(&AdmittedProjectionSession),
    ) -> CompletedExecution {
        let Self {
            coordinator,
            session,
            projection,
        } = self;
        let result = thread::scope(|scope| {
            let execution = scope.spawn(move || {
                let mut lifecycle = NoopLifecycle;
                let mut branch = NoopBranch;
                coordinator.execute_ordinary_turn(
                    fixture.store.home_for_shutdown_test(),
                    &fixture.storage,
                    &fixture.state.assets(),
                    None,
                    projection,
                    &fixture.cancellation,
                    request,
                    noop_handlers(&mut lifecycle, &mut branch),
                )
            });
            while_running(&session);
            execution.join().unwrap()
        });
        CompletedExecution { result, session }
    }

    pub fn execute_with_failed_service_disposal(
        self,
        fixture: Fixture,
        request: &OrdinaryTurnExecutionRequest,
        while_running: impl FnOnce(&AdmittedProjectionSession),
    ) -> (CompletedExecution, tempfile::TempDir) {
        use beryl_app::cas_projection::{
            PersistentFailureCutCompletion, PersistentFailureCutState,
            ProjectionConnectionServiceCloseOutcome,
            test_faults::capture_provider_broker_snapshot_reader,
        };
        let Self {
            coordinator,
            session,
            projection,
        } = self;
        let home = fixture.store.retain_home_for_shutdown_test();
        let storage = fixture.storage.clone();
        let assets = fixture.state.assets();
        let cancellation = fixture.cancellation.clone();
        let (directory, service) = fixture.into_service();
        let home_id = service.home_id();
        let home_generation = service.home_generation();
        let service_generation = service.service_generation();
        let retirement = session.connection_retirement_handle_for_test();
        let broker_metrics = capture_provider_broker_snapshot_reader(&session);
        let result = thread::scope(|scope| {
            let execution = scope.spawn(|| {
                let mut lifecycle = NoopLifecycle;
                let mut branch = NoopBranch;
                coordinator.execute_ordinary_turn(
                    &home,
                    &storage,
                    &assets,
                    None,
                    projection,
                    &cancellation,
                    request,
                    noop_handlers(&mut lifecycle, &mut branch),
                )
            });
            while_running(&session);
            let deadline = std::time::Instant::now() + TIMEOUT;
            while service.persistent_failure_cut_snapshot().state()
                != PersistentFailureCutState::Finished
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "failure cut did not finish"
                );
                thread::sleep(std::time::Duration::from_millis(10));
            }
            let ProjectionConnectionServiceCloseOutcome::PersistentFailure(evidence) =
                service.close().unwrap()
            else {
                panic!("failed-home disposal did not return persistent-failure evidence")
            };
            assert_eq!(evidence.home_id(), home_id);
            assert_eq!(evidence.home_generation(), home_generation);
            assert_eq!(evidence.service_generation(), service_generation);
            assert_eq!(
                evidence.completion(),
                PersistentFailureCutCompletion::Finished
            );
            assert_eq!(
                evidence.cut_snapshot().state(),
                PersistentFailureCutState::Finished
            );
            assert!(retirement.is_retired());
            assert!(retirement.is_detached());
            let deadline = std::time::Instant::now() + TIMEOUT;
            while !execution.is_finished() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "execution did not return after failed-service disposal"
                );
                thread::sleep(std::time::Duration::from_millis(10));
            }
            execution.join().unwrap()
        });
        let broker = broker_metrics.snapshot();
        assert_eq!(broker.in_flight().current(), 0);
        assert_eq!(broker.submitted(), broker.acked());
        assert_eq!(broker.staged_fragments().current(), 0);
        assert_eq!(broker.checked_user_publications().activity().current(), 0);
        let pages = session.provider_page_diagnostics();
        assert_eq!(pages.leased, 0);
        assert_eq!(pages.available, pages.page_count);
        (CompletedExecution { result, session }, directory)
    }
}

pub fn close_execution(session: AdmittedProjectionSession, server: RawCasServer) {
    session.invalidate_connection();
    drop(session);
    server.join();
}
