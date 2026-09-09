use std::path::Path;

use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequestHandler,
    cas_projection::{
        AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest,
        LoadedCasProjection, OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers,
        OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
    },
};
use beryl_backend::{
    DynamicToolCallResponse, ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions,
};
use beryl_model::CasProcessGeneration;
use syndic_storage::SyndicTimestamp;

use crate::{
    server::{AUTHORIZATION, TIMEOUT, YieldServer},
    syndic::{Fixture, execution_binding},
};

pub fn obtain(
    fixture: &Fixture,
    server: &YieldServer,
) -> (AdmittedProjectionSession, LoadedCasProjection) {
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(970_001).unwrap(),
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
                SyndicTimestamp::from_unix_millis(97_000),
                TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    (session, projection)
}

struct UnusedBranch;

impl BranchDiscussionResolutionRequestHandler for UnusedBranch {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        panic!("lifecycle execution must not dispatch a branch request")
    }
}

pub fn execute(
    fixture: &Fixture,
    projection: LoadedCasProjection,
    lifecycle: &mut dyn LifecycleYieldRequestHandler,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.home()).unwrap();
    coordinator.execute_ordinary_turn(
        &fixture.home(),
        &fixture.storage,
        &fixture.state.assets(),
        projection,
        &fixture.cancellation,
        &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
        OrdinaryDynamicToolHandlers::new(lifecycle, &mut UnusedBranch),
    )
}
