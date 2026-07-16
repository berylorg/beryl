#[path = "phase10_projection/backend.rs"]
mod backend;
#[path = "phase10_projection/connection.rs"]
mod connection;
#[path = "phase10_projection/native.rs"]
mod native;
#[path = "phase10_projection/recovery.rs"]
mod recovery;
#[path = "phase10_projection/syndic.rs"]
mod syndic;

use beryl_app::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, LoadedProjectionReleaseOutcome,
    NativeLineageOperation, ProjectionCancellationToken, ProjectionExecutionError,
};
use beryl_backend::{ThreadStartOptions, ThreadUnsubscribeStatus};
use beryl_model::{CasProcessGeneration, ExecutionBinding, RootId, RuntimeId};
use syndic_storage::{BindingState, CasLineageProof, NativeCasLineage, SyndicTimestamp};

use backend::{FakeAppServer, InjectionReply, ProjectionStep, TIMEOUT, UnsubscribeReply};
use syndic::{Fixture, execution_binding, point_limit};

fn process(value: u64) -> CasProcessGeneration {
    CasProcessGeneration::new(value).unwrap()
}

fn request(fixture: &Fixture, thread: beryl_model::SyndicThreadId) -> CasProjectionRequest {
    CasProjectionRequest::new(
        thread,
        fixture.selected_path(thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(1_000_000),
        SyndicTimestamp::from_unix_millis(10_000),
        TIMEOUT,
    )
}

fn alternate_root_binding() -> ExecutionBinding {
    let default = execution_binding();
    ExecutionBinding::new(
        default.runtime_id(),
        RootId::from_bytes([245; 16]),
        default.root_path().clone(),
    )
}

#[test]
fn fresh_native_projection_is_durable_before_reuse() {
    let mut fixture = Fixture::new(1);
    fixture.submit_text("root pending");
    let request = request(&fixture, fixture.thread);
    let server = FakeAppServer::spawn(vec![ProjectionStep::Fresh {
        target: "phase10-fresh-target",
    }]);
    let mut session = server.admit(execution_binding().runtime_id(), process(1));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();

    let first = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let durable = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = durable.binding().state() else {
        panic!("fresh projection must be durable before it is returned")
    };
    assert_eq!(durable.binding().revision(), first.binding_revision());
    assert_eq!(usable.cas_thread_id(), first.cas_thread_id());
    assert!(matches!(
        usable.lineage(),
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Fresh,
            ..
        }
    ));

    let second = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(second.cas_thread_id(), first.cas_thread_id());
    assert_eq!(second.binding_revision(), first.binding_revision());
    assert_eq!(
        second.loaded_session_generation(),
        first.loaded_session_generation()
    );
    server.join();
}
