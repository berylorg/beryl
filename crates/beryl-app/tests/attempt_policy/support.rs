use std::path::Path;

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    OrdinaryNotStartedProjection, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest, OrdinaryTurnNotStarted,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions};
use beryl_model::{CasProcessGeneration, SyndicThreadId};
use syndic_storage::SyndicTimestamp;

use super::{
    EXECUTION_ROOT, scheduler_support,
    server::{AUTHORIZATION, AttemptServer, TIMEOUT},
    syndic::{Fixture, execution_binding},
};

pub fn apply_instructions(fixture: &Fixture, value: &str) {
    fixture.apply_setting(
        beryl_app::settings_service::SettingsDraftValue::DeveloperInstructions(value.into()),
    );
}

pub fn admit(fixture: &Fixture, server: &AttemptServer, process: u64) -> AdmittedProjectionSession {
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    fixture
        .store
        .admit_runtime_lifecycle_test_candidate(
            &connector,
            execution_binding(),
            CasProcessGeneration::new(process).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap()
}

pub fn obtain(
    fixture: &Fixture,
    session: &mut AdmittedProjectionSession,
    thread: SyndicThreadId,
) -> LoadedCasProjection {
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            session,
            &CasProjectionRequest::new(
                thread,
                fixture.selected_path(thread),
                execution_binding(),
                ThreadStartOptions::persistent(),
                Some(1_000_000),
                SyndicTimestamp::from_unix_millis(95_000),
                TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap()
}

pub fn execute(
    fixture: &Fixture,
    projection: LoadedCasProjection,
    request: &OrdinaryTurnExecutionRequest,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let mut tools = scheduler_support::tool_authority();
    coordinator.execute_ordinary_turn(
        &*fixture.home(),
        &fixture.storage,
        &fixture.state.assets(),
        None,
        projection,
        &fixture.cancellation,
        request,
        tools.handlers(),
    )
}

pub fn rejected_projection(outcome: OrdinaryTurnExecutionOutcome) -> LoadedCasProjection {
    match outcome {
        OrdinaryTurnExecutionOutcome::NotStarted {
            projection: OrdinaryNotStartedProjection::Retained(projection),
            reason: OrdinaryTurnNotStarted::ExactRejection(_),
        } => *projection,
        other => panic!("expected retained exact rejection: {other:?}"),
    }
}
