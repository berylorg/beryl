#![cfg(feature = "test-faults")]

#[path = "loaded_thread_metadata/server.rs"]
mod server;
#[path = "projection/syndic.rs"]
mod syndic;

use std::path::Path;

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions};
use beryl_model::{CasProcessGeneration, SyndicThreadId};
use syndic_storage::SyndicTimestamp;

use server::{AUTHORIZATION, MetadataServer, TIMEOUT};
use syndic::{Fixture, execution_binding};

const EXECUTION_ROOT: &str = r"C:\work\beryl";

fn admit(fixture: &Fixture, server: &MetadataServer, process: u64) -> AdmittedProjectionSession {
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(process).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap()
}

fn obtain(
    fixture: &Fixture,
    session: &mut AdmittedProjectionSession,
    thread: SyndicThreadId,
) -> LoadedCasProjection {
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = CasProjectionRequest::new(
        thread,
        fixture.selected_path(thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(1_000_000),
        SyndicTimestamp::from_unix_millis(90_000),
        TIMEOUT,
    );
    coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            session,
            &request,
            &fixture.cancellation,
        )
        .unwrap()
}

fn assert_metadata(projection: &LoadedCasProjection, model: &str, reasoning: Option<&str>) {
    let metadata = projection.observed_thread_metadata().unwrap().unwrap();
    assert_eq!(metadata.model.as_deref(), Some(model));
    assert_eq!(metadata.model_provider.as_deref(), Some("openai"));
    assert_eq!(metadata.reasoning_effort.as_deref(), reasoning);
}

#[test]
fn fresh_metadata_survives_same_session_reuse_and_is_revoked_on_retirement() {
    let mut fixture = Fixture::new(171);
    fixture.submit_text("pending fresh metadata");
    let server = MetadataServer::spawn(
        "thread/start",
        None,
        "metadata-fresh",
        "fresh-model",
        Some("medium"),
    );
    let mut session = admit(&fixture, &server, 900_171);
    let first = obtain(&fixture, &mut session, fixture.thread);
    assert_metadata(&first, "fresh-model", Some("medium"));
    let second = obtain(&fixture, &mut session, fixture.thread);
    assert_eq!(
        first.loaded_session_generation(),
        second.loaded_session_generation()
    );
    assert_metadata(&second, "fresh-model", Some("medium"));
    drop(first);
    assert_metadata(&second, "fresh-model", Some("medium"));
    session.invalidate_connection();
    assert!(second.observed_thread_metadata().unwrap().is_none());
    drop(second);
    drop(session);
    server.join();
}

#[test]
fn unknown_reasoning_stays_absent_instead_of_using_root_defaults() {
    let mut fixture = Fixture::new(172);
    fixture.submit_text("pending unknown reasoning");
    let server = MetadataServer::spawn(
        "thread/start",
        None,
        "metadata-unknown",
        "unknown-effort-model",
        None,
    );
    let mut session = admit(&fixture, &server, 900_172);
    let projection = obtain(&fixture, &mut session, fixture.thread);
    assert_metadata(&projection, "unknown-effort-model", None);
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}

#[test]
fn resume_replacement_uses_new_response_and_cannot_revive_old_observation() {
    let mut fixture = Fixture::new(173);
    fixture.submit_text("pending metadata replacement");
    let server = MetadataServer::spawn(
        "thread/start",
        None,
        "metadata-replacement",
        "old-model",
        Some("low"),
    );
    let mut session = admit(&fixture, &server, 900_173);
    let old = obtain(&fixture, &mut session, fixture.thread);
    assert_metadata(&old, "old-model", Some("low"));
    session.invalidate_connection();
    drop(session);
    server.join();

    let server = MetadataServer::spawn(
        "thread/resume",
        Some("metadata-replacement".into()),
        "metadata-replacement",
        "new-model",
        Some("high"),
    );
    let mut replacement = admit(&fixture, &server, 900_174);
    let new = obtain(&fixture, &mut replacement, fixture.thread);
    assert_ne!(
        old.loaded_session_generation(),
        new.loaded_session_generation()
    );
    assert!(old.observed_thread_metadata().unwrap().is_none());
    assert_metadata(&new, "new-model", Some("high"));
    drop(old);
    assert_metadata(&new, "new-model", Some("high"));
    replacement.invalidate_connection();
    drop(new);
    drop(replacement);
    server.join();
}

#[test]
fn native_fork_preserves_target_response_metadata() {
    let mut fixture = Fixture::new(175);
    let completed = fixture.submit_text("parent history");
    fixture.complete_with_assistant(completed, "parent answer");
    let child = fixture.create_child_pending("child pending");
    let source = fixture
        .native_source(child)
        .binding()
        .cas_thread_id()
        .as_str()
        .to_owned();
    let server = MetadataServer::spawn(
        "thread/fork",
        Some(source),
        "metadata-child",
        "child-model",
        Some("minimal"),
    );
    let mut session = admit(&fixture, &server, 900_175);
    let projection = obtain(&fixture, &mut session, child);
    assert_metadata(&projection, "child-model", Some("minimal"));
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}
