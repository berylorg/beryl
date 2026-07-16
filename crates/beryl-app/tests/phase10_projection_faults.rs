#![cfg(feature = "test-faults")]

#[path = "phase10_projection/backend.rs"]
mod backend;
#[path = "phase10_projection/syndic.rs"]
mod syndic;

use beryl_app::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, ProjectionCancellationToken,
    ProjectionExecutionError, ProjectionPublicationFailure,
};
use beryl_backend::ThreadStartOptions;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_model::CasProcessGeneration;
use syndic_storage::{BindingState, SyndicTimestamp};

use backend::{FakeAppServer, InjectionReply, ProjectionStep, TIMEOUT};
use syndic::{Fixture, execution_binding, point_limit};

fn process(value: u64) -> CasProcessGeneration {
    CasProcessGeneration::new(value).unwrap()
}

fn request(fixture: &Fixture) -> CasProjectionRequest {
    CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(1_000_000),
        SyndicTimestamp::from_unix_millis(20_000),
        TIMEOUT,
    )
}

#[test]
fn post_persist_publication_ambiguity_never_returns_an_unproven_capability() {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(20, faults.clone());
    fixture.submit_text("root pending");
    let first_server = FakeAppServer::spawn(vec![ProjectionStep::Fresh {
        target: "phase10-ambiguous-persisted-target",
    }]);
    let mut first_session = first_server.admit(execution_binding().runtime_id(), process(20));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    faults.fail_next(FaultPoint::AfterPersist);

    let error = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut first_session,
            &request(&fixture),
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err();
    let ProjectionExecutionError::AbandonmentFailed { publication, .. } = error else {
        panic!("ambiguous publication must report failed abandonment")
    };
    assert!(matches!(
        publication.as_deref(),
        Some(ProjectionPublicationFailure::HomeAuthorityLost(_))
    ));
    first_server.join();
    drop(first_session);

    fixture.store.verify_health().unwrap();
    let durable = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = durable.binding().state() else {
        panic!("post-persist verification must reveal the whole valid publication")
    };
    assert_eq!(
        usable.cas_thread_id().as_str(),
        "phase10-ambiguous-persisted-target"
    );

    let second_server = FakeAppServer::spawn(vec![ProjectionStep::Resume {
        source: usable.cas_thread_id().as_str().to_string(),
    }]);
    let mut second_session = second_server.admit(execution_binding().runtime_id(), process(21));
    let recovered_coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = recovered_coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut second_session,
            &request(&fixture),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(
        projection.cas_thread_id().as_str(),
        "phase10-ambiguous-persisted-target"
    );
    second_server.join();
}

#[test]
fn ambiguous_recovered_publication_never_reuses_the_injected_target_after_registry_loss() {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(21, faults.clone());
    let history = fixture.submit_text("history user");
    fixture.complete_with_assistant(history, "history assistant");
    fixture.submit_text("pending user");
    fixture.retire_current_binding(fixture.thread);
    let first_server = FakeAppServer::spawn(vec![ProjectionStep::Recover {
        target: "phase10-ambiguous-recovered-target",
        injection: InjectionReply::Success,
    }]);
    let mut first_session = first_server.admit(execution_binding().runtime_id(), process(22));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    faults.fail_next(FaultPoint::AfterPersist);

    let error = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut first_session,
            &request(&fixture),
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err();
    let ProjectionExecutionError::AbandonmentFailed { publication, .. } = error else {
        panic!("ambiguous publication must report failed abandonment")
    };
    assert!(matches!(
        publication.as_deref(),
        Some(ProjectionPublicationFailure::HomeAuthorityLost(_))
    ));
    first_server.join();
    drop(first_session);
    fixture.store.verify_health().unwrap();

    let persisted = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(persisted) = persisted.binding().state() else {
        panic!("post-persist recovery proof must validate as a whole binding")
    };
    assert_eq!(
        persisted.cas_thread_id().as_str(),
        "phase10-ambiguous-recovered-target"
    );

    let second_server = FakeAppServer::spawn(vec![ProjectionStep::Recover {
        target: "phase10-recovered-after-ambiguity",
        injection: InjectionReply::Success,
    }]);
    let mut second_session = second_server.admit(execution_binding().runtime_id(), process(23));
    let recovered_coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = recovered_coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut second_session,
            &request(&fixture),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(
        projection.cas_thread_id().as_str(),
        "phase10-recovered-after-ambiguity"
    );
    second_server.join();
}
