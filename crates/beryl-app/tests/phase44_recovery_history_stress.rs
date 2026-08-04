#![cfg(feature = "test-faults")]

#[path = "phase44_recovery_history_stress/history.rs"]
mod history;
#[allow(dead_code, unused_imports)]
#[path = "phase38_submitted_input_residency/wire.rs"]
mod raw_wire;
#[path = "phase44_recovery_history_stress/server.rs"]
mod server;
#[path = "phase10_projection/syndic.rs"]
mod syndic;
#[path = "phase44_recovery_history_stress/verification.rs"]
mod verification;

use std::{
    path::Path,
    sync::{Arc, Mutex},
    thread,
};

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest,
    ProjectionExecutionError,
    test_faults::{install_recovery_page_handoff_barrier, install_recovery_source_barrier},
};
use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendError, ThreadInjectionSourceError,
    ThreadStartOptions,
};
use beryl_model::CasProcessGeneration;
use syndic_storage::{
    RecoveryBudgetKind, RecoveryProjectionError, SyndicTimestamp,
    test_faults::reset_recovery_residency_metrics,
};

use history::{HistorySpec, InstalledHistory, MODEL_BOUNDARY_TOKENS, PRODUCT_UTF8_LIMIT};
use server::{AUTHORIZATION, RecoveryServer, TIMEOUT, target_thread};
use syndic::{Fixture, execution_binding};
use verification::{
    assert_failed_recovery_is_stale, assert_live_capacity_one, assert_recovered_lineage,
    assert_recovery_plateau, assert_recovery_released, assert_syndic_constant_residency,
};

static PHASE44_TEST_LOCK: Mutex<()> = Mutex::new(());

const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[test]
fn exact_limit_fragmented_and_deep_histories_stream_with_constant_local_residency() {
    let _guard = PHASE44_TEST_LOCK.lock().unwrap();
    let histories = Arc::new(vec![
        HistorySpec::fragmented_limit(),
        HistorySpec::deep_limit(),
    ]);
    let mut fixture = Fixture::new(144);
    let first_thread = fixture.thread;
    let first = histories[0].install(&mut fixture, first_thread, "pending fragmented recovery");
    let second_thread = fixture.create_ordinary(145);
    let second = histories[1].install(&mut fixture, second_thread, "pending deep recovery");

    let server = RecoveryServer::spawn_success(Arc::clone(&histories));
    let mut session = admit(&fixture, &server, 440_001);
    let observer = session.recovery_replay_diagnostics_observer();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();

    let first_expected =
        histories[0].prepare_exact_projection(&fixture, first, MODEL_BOUNDARY_TOKENS);
    reset_recovery_residency_metrics();
    let first_barrier = install_recovery_page_handoff_barrier(first.thread, 0);
    let first_request = request(first, MODEL_BOUNDARY_TOKENS, 44_000);
    let first_projection = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                &fixture.store,
                fixture.storage,
                &mut session,
                &first_request,
                &fixture.cancellation,
            )
        });
        first_barrier.wait();
        assert_eq!(
            fixture.storage.revision(&fixture.store).unwrap(),
            first_expected.source_revision(),
            "first cursor replay must remain bound to the exact prepared Syndic revision"
        );
        assert_live_capacity_one(
            observer.snapshot().expect("first recovery diagnostics"),
            0,
            0,
            0,
        );
        first_barrier.release();
        worker.join().unwrap().unwrap()
    });
    let first_recovery = observer.snapshot().expect("first recovery retained");
    assert_recovery_released(
        first_recovery,
        histories[0].expected_pages(),
        histories[0].item_count(),
        histories[0].utf8_bytes(),
    );
    assert_syndic_constant_residency(histories[0].expected_pages(), histories[0].item_count());
    let first_proof = assert_recovered_lineage(
        &fixture,
        first,
        &histories[0],
        first_expected,
        &first_projection,
        &target_thread(1),
    );
    let second_expected =
        histories[1].prepare_exact_projection(&fixture, second, MODEL_BOUNDARY_TOKENS);
    reset_recovery_residency_metrics();
    let second_barrier = install_recovery_page_handoff_barrier(second.thread, 0);
    let second_request = request(second, MODEL_BOUNDARY_TOKENS, 44_001);
    let second_projection = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                &fixture.store,
                fixture.storage,
                &mut session,
                &second_request,
                &fixture.cancellation,
            )
        });
        second_barrier.wait();
        assert_eq!(
            fixture.storage.revision(&fixture.store).unwrap(),
            second_expected.source_revision(),
            "second cursor replay must remain bound to the exact prepared Syndic revision"
        );
        assert_live_capacity_one(
            observer.snapshot().expect("second recovery diagnostics"),
            0,
            0,
            0,
        );
        second_barrier.release();
        worker.join().unwrap().unwrap()
    });
    let second_recovery = observer.snapshot().expect("second recovery retained");
    assert_recovery_released(
        second_recovery,
        histories[1].expected_pages(),
        histories[1].item_count(),
        histories[1].utf8_bytes(),
    );
    assert_syndic_constant_residency(histories[1].expected_pages(), histories[1].item_count());
    let second_proof = assert_recovered_lineage(
        &fixture,
        second,
        &histories[1],
        second_expected,
        &second_projection,
        &target_thread(2),
    );
    assert_ne!(
        first_projection.cas_thread_id(),
        second_projection.cas_thread_id()
    );
    assert_ne!(
        first_proof.loaded_generation(),
        second_proof.loaded_generation()
    );
    assert_recovery_plateau(first_recovery, second_recovery);
    session.invalidate_connection();
    drop(second_projection);
    drop(first_projection);
    drop(session);
    server.join();
}

#[test]
fn exact_model_and_product_budget_overflow_is_rejected_before_remote_dispatch() {
    let _guard = PHASE44_TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(146);
    let model_history = HistorySpec::exact_budget_boundary();
    let model_thread = fixture.thread;
    let model = model_history.install(&mut fixture, model_thread, "pending model rejection");
    let product_history = HistorySpec::product_overflow();
    let product_thread = fixture.create_ordinary(147);
    let product =
        product_history.install(&mut fixture, product_thread, "pending product rejection");

    let server = RecoveryServer::spawn_no_dispatch();
    let mut session = admit(&fixture, &server, 440_002);
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();

    let model_error = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(model, MODEL_BOUNDARY_TOKENS - 1, 44_010),
            &fixture.cancellation,
        )
        .unwrap_err();
    assert_budget_overflow(model_error, PRODUCT_UTF8_LIMIT - 1, PRODUCT_UTF8_LIMIT);

    let product_error = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(product, u64::MAX, 44_011),
            &fixture.cancellation,
        )
        .unwrap_err();
    assert_budget_overflow(product_error, PRODUCT_UTF8_LIMIT, PRODUCT_UTF8_LIMIT + 1);
    assert!(
        session.recovery_replay_diagnostics().is_none(),
        "budget rejection must precede recovery-broker allocation"
    );
    session.invalidate_connection();
    drop(session);
    server.join();
}

#[test]
fn cancellation_before_injection_dispatch_is_proven_and_releases_capacity_one_broker() {
    let _guard = PHASE44_TEST_LOCK.lock().unwrap();
    let history = HistorySpec::fragmented_limit();
    let mut fixture = Fixture::new(148);
    let fixture_thread = fixture.thread;
    let installed = history.install(
        &mut fixture,
        fixture_thread,
        "pending predispatch cancellation",
    );

    let server = RecoveryServer::spawn_predispatch_cancellation();
    let mut session = admit(&fixture, &server, 440_003);
    let observer = session.recovery_replay_diagnostics_observer();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let request = request(installed, MODEL_BOUNDARY_TOKENS, 44_020);
    let barrier = install_recovery_source_barrier(installed.thread, 0);

    let error = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                &fixture.store,
                fixture.storage,
                &mut session,
                &request,
                &fixture.cancellation,
            )
        });
        barrier.wait();
        assert_live_capacity_one(
            observer.snapshot().expect("predispatch diagnostics"),
            0,
            0,
            0,
        );
        fixture.cancellation.cancel();
        barrier.release();
        worker.join().unwrap().unwrap_err()
    });

    assert_cancelled(error, false);
    assert_failed_recovery_is_stale(&fixture, installed, &target_thread(1));
    assert_recovery_released(
        observer.snapshot().expect("predispatch final diagnostics"),
        0,
        0,
        0,
    );

    session.invalidate_connection();
    drop(session);
    server.join();
}

#[test]
fn cancellation_after_first_transport_frame_is_completion_unknown_and_releases() {
    let _guard = PHASE44_TEST_LOCK.lock().unwrap();
    let history = Arc::new(HistorySpec::fragmented_limit());
    let mut fixture = Fixture::new(149);
    let fixture_thread = fixture.thread;
    let installed = history.install(
        &mut fixture,
        fixture_thread,
        "pending post-byte cancellation",
    );

    let server = RecoveryServer::spawn_post_first_page_cancellation(Arc::clone(&history));
    let mut session = admit(&fixture, &server, 440_004);
    let observer = session.recovery_replay_diagnostics_observer();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let request = request(installed, MODEL_BOUNDARY_TOKENS, 44_030);
    let barrier = install_recovery_source_barrier(installed.thread, 1);

    let error = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                &fixture.store,
                fixture.storage,
                &mut session,
                &request,
                &fixture.cancellation,
            )
        });
        barrier.wait();
        assert_live_capacity_one(
            observer.snapshot().expect("post-byte diagnostics"),
            1,
            0,
            65_535,
        );
        fixture.cancellation.cancel();
        barrier.release();
        worker.join().unwrap().unwrap_err()
    });

    assert_cancelled(error, true);
    assert_failed_recovery_is_stale(&fixture, installed, &target_thread(1));
    assert_recovery_released(
        observer.snapshot().expect("post-byte final diagnostics"),
        1,
        0,
        65_535,
    );

    session.invalidate_connection();
    drop(session);
    server.join();
}

fn admit(fixture: &Fixture, server: &RecoveryServer, generation: u64) -> AdmittedProjectionSession {
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    fixture
        .store
        .admit(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(generation).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap()
}

fn request(
    installed: InstalledHistory,
    model_context_window_tokens: u64,
    observed_at: u64,
) -> CasProjectionRequest {
    CasProjectionRequest::new(
        installed.thread,
        installed.selected_path,
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(model_context_window_tokens),
        SyndicTimestamp::from_unix_millis(observed_at),
        TIMEOUT,
    )
}

fn assert_budget_overflow(error: ProjectionExecutionError, maximum: u64, actual: u64) {
    let ProjectionExecutionError::RecoveryProjection(RecoveryProjectionError::BudgetOverflow {
        kind,
        maximum: observed_maximum,
        actual: observed_actual,
    }) = error
    else {
        panic!("unexpected predispatch budget outcome: {error:?}")
    };
    assert_eq!(kind, RecoveryBudgetKind::Utf8Bytes);
    assert_eq!(observed_maximum, maximum);
    assert_eq!(observed_actual, actual);
}

fn assert_cancelled(error: ProjectionExecutionError, transport_bytes_written: bool) {
    let (thread_id, source) = match error {
        ProjectionExecutionError::InjectionNotDispatched { thread_id, source }
            if !transport_bytes_written =>
        {
            (thread_id, source)
        }
        ProjectionExecutionError::InjectionCompletionUnknown { thread_id, source }
            if transport_bytes_written =>
        {
            (thread_id, source)
        }
        error => panic!("unexpected recovery cancellation outcome: {error:?}"),
    };
    assert_eq!(thread_id.as_str(), target_thread(1));
    assert!(matches!(
        *source,
        ManagedBackendError::ThreadInjectionSource {
            source: ThreadInjectionSourceError::Cancelled,
            transport_bytes_written: observed,
            ..
        } if observed == transport_bytes_written
    ));
}
