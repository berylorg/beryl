use std::path::Path;

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions};
use beryl_model::CasProcessGeneration;
use syndic_storage::SyndicTimestamp;

use crate::{
    scheduler_support,
    server::{AUTHORIZATION, CompactionServer, TIMEOUT},
    syndic::{Fixture, execution_binding},
};

pub fn obtain(
    fixture: &Fixture,
    server: &CompactionServer,
) -> (AdmittedProjectionSession, LoadedCasProjection) {
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(960_001).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.home()).unwrap();
    let projection = coordinator
        .obtain_projection(
            &fixture.home(),
            &fixture.storage,
            &mut session,
            &CasProjectionRequest::new(
                fixture.thread,
                fixture.selected_path(fixture.thread),
                execution_binding(),
                ThreadStartOptions::persistent(),
                Some(1_000_000),
                SyndicTimestamp::from_unix_millis(95_000),
                TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    (session, projection)
}

pub fn execute(
    fixture: &Fixture,
    projection: LoadedCasProjection,
    request: &OrdinaryTurnExecutionRequest,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.home()).unwrap();
    let mut tools = scheduler_support::tool_authority();
    coordinator.execute_ordinary_turn(
        &fixture.home(),
        &fixture.storage,
        &fixture.state.assets(),
        projection,
        &fixture.cancellation,
        request,
        tools.handlers(),
    )
}
