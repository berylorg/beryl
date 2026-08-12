#![cfg(feature = "test-faults")]

#[allow(dead_code, unused_imports)]
#[path = "phase38_submitted_input_residency/wire.rs"]
mod raw_wire;
#[path = "phase40_recovery_page_residency/server.rs"]
mod server;
#[path = "phase10_projection/syndic.rs"]
mod syndic;

use std::{path::Path, sync::Mutex, thread};

use beryl_app::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, ProjectionExecutionError,
    RecoveryReplayDiagnosticsSnapshot,
    test_faults::{install_recovery_page_handoff_barrier, install_recovery_source_barrier},
};
use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendError, ThreadInjectionSourceError,
    ThreadStartOptions,
};
use beryl_model::CasProcessGeneration;
use syndic_storage::{CasLineageProof, SyndicTimestamp};

use server::{AUTHORIZATION, RecoveryServer, TARGET_THREAD, TIMEOUT};
use syndic::{Fixture, execution_binding};

const EXECUTION_ROOT: &str = r"C:\work\beryl";
const HISTORY_USER: &str = "history user";
const HISTORY_ASSISTANT: &str = "history assistant";

static PHASE40_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn production_recovery_rendezvous_holds_one_fixed_page_then_releases() {
    let _guard = PHASE40_TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(140);
    let completed = fixture.submit_text(HISTORY_USER);
    fixture.complete_with_assistant(completed, HISTORY_ASSISTANT);
    fixture.submit_text("pending user");
    fixture.retire_current_binding(fixture.thread);

    let server = RecoveryServer::spawn(HISTORY_USER, HISTORY_ASSISTANT);
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(400_001).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let observer = session.recovery_replay_diagnostics_observer();
    assert!(observer.snapshot().is_none());

    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(1_000_000),
        SyndicTimestamp::from_unix_millis(40_000),
        TIMEOUT,
    );
    let barrier = install_recovery_page_handoff_barrier(fixture.thread, 0);

    let projection = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                &*fixture.home(),
                fixture.storage,
                &mut session,
                &request,
                &fixture.cancellation,
            )
        });
        barrier.wait();
        assert_live_handoff(observer.snapshot().expect("recovery diagnostics published"));
        barrier.release();
        worker.join().unwrap().unwrap()
    });

    assert_eq!(projection.cas_thread_id().as_str(), TARGET_THREAD);
    assert!(matches!(
        projection.lineage_proof(),
        CasLineageProof::RecoveredInjection(_)
    ));
    assert_released(
        observer
            .snapshot()
            .expect("final recovery diagnostics retained"),
    );

    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}

#[test]
fn production_recovery_cancellation_returns_typed_failure_and_releases() {
    let _guard = PHASE40_TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(141);
    let completed = fixture.submit_text(HISTORY_USER);
    fixture.complete_with_assistant(completed, HISTORY_ASSISTANT);
    fixture.submit_text("pending cancellation");
    fixture.retire_current_binding(fixture.thread);

    let server = RecoveryServer::spawn_cancellation();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(400_002).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let observer = session.recovery_replay_diagnostics_observer();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(1_000_000),
        SyndicTimestamp::from_unix_millis(40_001),
        TIMEOUT,
    );
    let barrier = install_recovery_source_barrier(fixture.thread, 0);

    let error = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                &*fixture.home(),
                fixture.storage,
                &mut session,
                &request,
                &fixture.cancellation,
            )
        });
        barrier.wait();
        assert_live_handoff(observer.snapshot().expect("recovery diagnostics published"));
        fixture.cancellation.cancel();
        barrier.release();
        worker.join().unwrap().unwrap_err()
    });

    assert_cancelled(error);
    assert_cancelled_released(
        observer
            .snapshot()
            .expect("cancelled recovery diagnostics retained"),
    );

    session.invalidate_connection();
    drop(session);
    server.join();
}

fn assert_live_handoff(snapshot: RecoveryReplayDiagnosticsSnapshot) {
    assert!(!snapshot.released());
    assert!(snapshot.final_capacity().is_none());
    assert_eq!(snapshot.logical_pages(), 0);
    assert_eq!(snapshot.logical_items(), 0);
    assert_eq!(snapshot.logical_utf8_bytes(), 0);

    let capacity = snapshot
        .live_capacity()
        .expect("page and both rings are live at handoff");
    let pages = capacity.pages();
    assert_eq!(pages.page_capacity, 65_536);
    assert_eq!(pages.page_count, 1);
    assert_eq!(pages.available, 0);
    assert_eq!(pages.leased, 1);
    assert_eq!(pages.high_water, 1);
    assert_eq!(pages.total_leases, 1);
    assert_eq!(pages.exhausted, 0);

    let requests = capacity.requests();
    assert_eq!(requests.capacity, 1);
    assert_eq!(requests.len, 0);
    assert_eq!(requests.sends, 1);
    assert_eq!(requests.receives, 1);
    assert_eq!(requests.high_water, 1);
    assert!(requests.sender_open && requests.receiver_open);

    let replies = capacity.replies();
    assert_eq!(replies.capacity, 1);
    assert_eq!(replies.len, 0);
    assert_eq!(replies.sends, 0);
    assert_eq!(replies.receives, 0);
    assert_eq!(replies.high_water, 0);
    assert!(replies.sender_open && replies.receiver_open);
}

fn assert_released(snapshot: RecoveryReplayDiagnosticsSnapshot) {
    assert!(snapshot.released());
    assert!(snapshot.live_capacity().is_none());
    assert_eq!(snapshot.logical_pages(), 2);
    assert_eq!(snapshot.logical_items(), 2);
    assert_eq!(
        snapshot.logical_utf8_bytes(),
        u64::try_from(HISTORY_USER.len() + HISTORY_ASSISTANT.len()).unwrap()
    );

    let capacity = snapshot
        .final_capacity()
        .expect("final diagnostics captured before ownership release");
    let pages = capacity.pages();
    assert_eq!(pages.page_capacity, 65_536);
    assert_eq!(pages.page_count, 1);
    assert_eq!(pages.available, 1);
    assert_eq!(pages.leased, 0);
    assert_eq!(pages.high_water, 1);
    assert!(pages.total_leases >= 2);
    assert_eq!(pages.exhausted, 0);

    for channel in [capacity.requests(), capacity.replies()] {
        assert_eq!(channel.capacity, 1);
        assert_eq!(channel.len, 0);
        assert_eq!(channel.high_water, 1);
    }
}

fn assert_cancelled(error: ProjectionExecutionError) {
    let ProjectionExecutionError::InjectionNotDispatched { thread_id, source } = error else {
        panic!("unexpected recovery cancellation outcome: {error:?}")
    };
    assert_eq!(thread_id.as_str(), TARGET_THREAD);
    assert!(matches!(
        *source,
        ManagedBackendError::ThreadInjectionSource {
            source: ThreadInjectionSourceError::Cancelled,
            transport_bytes_written: false,
            ..
        }
    ));
}

fn assert_cancelled_released(snapshot: RecoveryReplayDiagnosticsSnapshot) {
    assert!(snapshot.released());
    assert!(snapshot.live_capacity().is_none());
    assert_eq!(snapshot.logical_pages(), 0);
    assert_eq!(snapshot.logical_items(), 0);
    assert_eq!(snapshot.logical_utf8_bytes(), 0);

    let capacity = snapshot
        .final_capacity()
        .expect("cancelled recovery captured final ownership facts");
    let pages = capacity.pages();
    assert_eq!(pages.page_count, 1);
    assert_eq!(pages.available, 1);
    assert_eq!(pages.leased, 0);
    assert_eq!(pages.high_water, 1);
    assert_eq!(pages.total_leases, 1);
    assert_eq!(pages.exhausted, 0);
    for channel in [capacity.requests(), capacity.replies()] {
        assert_eq!(channel.capacity, 1);
        assert_eq!(channel.len, 0);
        assert_eq!(channel.high_water, 1);
    }
}
