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
            .admit(
                &connector,
                execution_binding().runtime_id(),
                generation,
                Path::new(EXECUTION_ROOT),
                TIMEOUT,
            )
            .unwrap();
        let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
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
                &fixture.store,
                fixture.storage,
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
                    &fixture.store,
                    fixture.storage,
                    fixture.state.assets(),
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
}

pub fn close_execution(session: AdmittedProjectionSession, server: RawCasServer) {
    session.invalidate_connection();
    drop(session);
    server.join();
}
