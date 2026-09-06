#![cfg(feature = "test-faults")]

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[path = "native_lineage_scheduler/scheduler.rs"]
mod scheduler_support;
#[path = "native_lineage_scheduler/support.rs"]
mod server_support;
#[path = "native_lineage_scheduler/syndic.rs"]
mod syndic;

use std::{num::NonZeroUsize, path::Path};

use beryl_app::cas_projection::{
    DURABLE_START_ADMISSION_BUDGET_BYTES, NativeLineageOperation, NativeLineageRecoveryCommand,
    NativeLineageRecoveryCommandError, NativeLineageRecoveryControl, NativeLineageRecoveryStatus,
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_home_store::test_faults::{FaultController, FreeSpaceTestObservation};
use beryl_model::{BindingRevision, CasProcessGeneration, RuntimeId, SyndicThreadId, SyndicTurnId};
use serde_json::json;
use syndic_storage::{AcceptedRouteEffectiveState, SyndicStorage, TurnLifecycle};

use scheduler_support::{
    NextRecordIds, SessionPool, SessionSlot, accepted_route_state,
    admit_runtime_next_input_after_direct_setup, current_cas_thread_id, point_limit,
    pooled_ready_provider, ready_provider,
    seed_runtime_next_input_on_without_wake_after_direct_setup, wait_until,
};
use server_support::{AUTHORIZATION, NativeLineageServer, TIMEOUT};

fn scheduler_fixture(
    seed: u8,
    worker_capacity: u64,
) -> (syndic::Fixture, FaultController, SessionSlot, Box<str>) {
    let faults = FaultController::new();
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider_faults_and_capacity(
        seed,
        faults.clone(),
        worker_capacity,
        move |assets| Box::new(ready_provider(provider_slot, assets)),
    );
    let parent = fixture.submit_text(" completed parent");
    fixture.complete_with_assistant(parent, " completed answer");
    let cas_thread_id = {
        let command_home = fixture.store.live_home_command().unwrap();
        current_cas_thread_id(command_home.home(), fixture.storage.clone(), fixture.thread)
    };
    assert!(
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .recovery_handed_off()
    );
    (fixture, faults, slot, cas_thread_id.into())
}

fn pooled_scheduler_fixture(
    seed: u8,
    worker_capacity: u64,
) -> (syndic::Fixture, FaultController, SessionPool) {
    let faults = FaultController::new();
    let pool = SessionPool::default();
    let provider_pool = pool.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider_faults_and_capacity(
        seed,
        faults.clone(),
        worker_capacity,
        move |assets| Box::new(pooled_ready_provider(provider_pool, assets)),
    );
    let parent = fixture.submit_text(" pooled completed parent");
    fixture.complete_with_assistant(parent, " pooled completed answer");
    assert!(
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .recovery_handed_off()
    );
    (fixture, faults, pool)
}

fn attach_session(
    fixture: &syndic::Fixture,
    slot: &SessionSlot,
    server: &NativeLineageServer,
    generation: u64,
) {
    let execution = syndic::execution_binding();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(generation).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
}

fn attach_pooled_session(
    fixture: &syndic::Fixture,
    pool: &SessionPool,
    server: &NativeLineageServer,
    runtime_id: RuntimeId,
    generation: u64,
) {
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(generation).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    pool.push(session);
}

fn prepare_completed_thread(
    fixture: &mut syndic::Fixture,
    seed: u8,
    text: &str,
) -> (SyndicThreadId, RuntimeId) {
    let runtime_id = RuntimeId::from_bytes([seed.wrapping_add(64); 16]);
    let thread = fixture.create_ordinary_with_execution(
        seed,
        scheduler_support::execution_binding(runtime_id.clone()),
    );
    let parent = fixture.submit_text_on(thread, text);
    fixture.complete_with_assistant_on(thread, parent, " pooled completed answer");
    (thread, runtime_id)
}

fn seed_pending_turn_on(
    fixture: &mut syndic::Fixture,
    faults: &FaultController,
    thread: SyndicThreadId,
    seed: u8,
) -> NextRecordIds {
    let required = DURABLE_START_ADMISSION_BUDGET_BYTES + 1;
    seed_runtime_next_input_on_without_wake_after_direct_setup(fixture, thread, seed, || {
        faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
            available_bytes: required,
            total_free_bytes: required,
            total_bytes: required,
        });
    })
}

fn admit_pending_turn(
    fixture: &mut syndic::Fixture,
    faults: &FaultController,
    seed: u8,
) -> NextRecordIds {
    let required = DURABLE_START_ADMISSION_BUDGET_BYTES + 1;
    admit_runtime_next_input_after_direct_setup(fixture, seed, || {
        faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
            available_bytes: required,
            total_free_bytes: required,
            total_bytes: required,
        });
    })
}

fn promoted_pending_turn(
    service: &beryl_app::cas_projection::ProjectionConnectionService,
    storage: &SyndicStorage,
    ids: &NextRecordIds,
) -> SyndicTurnId {
    let command_home = service.live_home_command().unwrap();
    assert_eq!(
        accepted_route_state(command_home.home(), storage.clone(), ids),
        AcceptedRouteEffectiveState::Promoted
    );
    let pending = storage
        .thread(command_home.home(), ids.thread, point_limit())
        .unwrap()
        .unwrap()
        .committed_tail()
        .unwrap();
    assert_ne!(pending, ids.parent);
    pending
}

fn wait_for_parked_route(
    fixture: &syndic::Fixture,
) -> beryl_app::cas_projection::NativeLineageRecoverySnapshot {
    let control = fixture.store.native_lineage_recovery_control();
    wait_until("scheduler-originated native-lineage route", || {
        control.snapshot_for_thread(fixture.thread)
    })
}

fn wait_for_parked_route_on(
    fixture: &syndic::Fixture,
    thread: SyndicThreadId,
) -> beryl_app::cas_projection::NativeLineageRecoverySnapshot {
    let control = fixture.store.native_lineage_recovery_control();
    wait_until("scheduler-originated native-lineage route", || {
        control.snapshot_for_thread(thread)
    })
}

fn wait_for_turn_complete(fixture: &syndic::Fixture, pending_turn: SyndicTurnId) {
    wait_until("retained pending turn completion", || {
        let command_home = fixture.store.live_home_command().ok()?;
        let state = fixture
            .storage
            .turn_state(command_home.home(), pending_turn, point_limit())
            .ok()
            .flatten()?;
        (state.lifecycle() == TurnLifecycle::Complete).then_some(())
    });
}

fn close_fixture(fixture: syndic::Fixture, slot: &SessionSlot, server: NativeLineageServer) {
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn scheduler_parks_without_worker_custody_and_retry_continues_the_exact_turn_once() {
    let (mut fixture, faults, slot, cas_thread_id) = scheduler_fixture(234, 4);
    let server = NativeLineageServer::spawn_retry_success(cas_thread_id);
    attach_session(&fixture, &slot, &server, 234_001);
    let ids = admit_pending_turn(&mut fixture, &faults, 234);

    server.wait_for_first_resume();
    let pending_turn = promoted_pending_turn(&fixture.store, &fixture.storage, &ids);
    server.release_first_resume();
    server.wait_for_initial_retries();
    let initial = wait_for_parked_route(&fixture);
    wait_until("parked worker permit release", || {
        (fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0)
            .then_some(())
    });
    assert_eq!(
        initial.status(),
        NativeLineageRecoveryStatus::Ready {
            recovery_available: true,
        }
    );
    assert_eq!(
        promoted_pending_turn(&fixture.store, &fixture.storage, &ids),
        pending_turn
    );

    let control = fixture.store.native_lineage_recovery_control();
    control
        .submit(initial.key(), NativeLineageRecoveryCommand::Retry)
        .unwrap();
    server.wait_for_turn_start();
    wait_for_turn_complete(&fixture, pending_turn);
    wait_until("retry worker release", || {
        (fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0)
            .then_some(())
    });
    let leaving = control.snapshot_for_thread(fixture.thread).unwrap();
    assert_eq!(leaving.key(), initial.key());
    assert_eq!(
        leaving.status(),
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true,
        }
    );
    assert_eq!(
        promoted_pending_turn(&fixture.store, &fixture.storage, &ids),
        pending_turn
    );
    control.acknowledge_leaving(initial.key()).unwrap();
    assert!(control.snapshot_for_thread(fixture.thread).is_none());

    close_fixture(fixture, &slot, server);
}

#[test]
fn scheduler_retry_failure_reissues_the_same_route_then_recovery_avoids_replay() {
    let (mut fixture, faults, slot, cas_thread_id) = scheduler_fixture(235, 4);
    let server = NativeLineageServer::spawn_retry_failure_then_recovery(cas_thread_id);
    attach_session(&fixture, &slot, &server, 234_002);
    let ids = admit_pending_turn(&mut fixture, &faults, 235);

    server.wait_for_first_resume();
    let pending_turn = promoted_pending_turn(&fixture.store, &fixture.storage, &ids);
    server.release_first_resume();
    server.wait_for_initial_retries();
    let initial = wait_for_parked_route(&fixture);
    let control = fixture.store.native_lineage_recovery_control();
    control
        .submit(initial.key(), NativeLineageRecoveryCommand::Retry)
        .unwrap();
    server.wait_for_command_retries();
    let failed = wait_until("failed retry route reissue", || {
        let snapshot = control.snapshot_for_thread(fixture.thread)?;
        matches!(
            snapshot.status(),
            NativeLineageRecoveryStatus::Failed {
                command: NativeLineageRecoveryCommand::Retry,
                ..
            }
        )
        .then_some(snapshot)
    });
    assert_eq!(failed.key(), initial.key());
    assert_eq!(failed.source_thread_id(), initial.source_thread_id());
    assert_eq!(
        failed.source_binding_revision(),
        initial.source_binding_revision()
    );
    assert_eq!(failed.operation(), initial.operation());
    assert_eq!(failed.failed_attempts(), initial.failed_attempts());
    wait_until("failed retry worker release", || {
        (fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0)
            .then_some(())
    });
    assert_eq!(
        promoted_pending_turn(&fixture.store, &fixture.storage, &ids),
        pending_turn
    );

    control
        .submit(
            failed.key(),
            NativeLineageRecoveryCommand::RecoverFromSyndic,
        )
        .unwrap();
    let injected = server.wait_for_recovery_injection();
    assert_eq!(
        injected,
        vec![
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":" completed parent"}]}),
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":" completed answer"}]}),
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":" non-steerable predecessor"}]})
        ]
    );
    assert!(
        !injected
            .iter()
            .any(|item| item.to_string().contains(scheduler_support::SUBMITTED_TEXT))
    );
    server.wait_for_turn_start();
    wait_for_turn_complete(&fixture, pending_turn);
    let leaving = control.snapshot_for_thread(fixture.thread).unwrap();
    assert_eq!(leaving.key(), initial.key());
    assert_eq!(
        leaving.status(),
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true,
        }
    );
    assert_eq!(
        promoted_pending_turn(&fixture.store, &fixture.storage, &ids),
        pending_turn
    );
    control.acknowledge_leaving(initial.key()).unwrap();

    close_fixture(fixture, &slot, server);
}

#[test]
fn coalesced_native_lineage_commands_both_run_their_exact_retained_turns() {
    let (mut fixture, faults, pool) = pooled_scheduler_fixture(245, 8);
    let first_thread = fixture.thread;
    let (second_thread, second_runtime) =
        prepare_completed_thread(&mut fixture, 246, " second parent");
    let first_ids = seed_pending_turn_on(&mut fixture, &faults, first_thread, 245);
    let second_ids = seed_pending_turn_on(&mut fixture, &faults, second_thread, 246);
    let first_server = NativeLineageServer::spawn_retry_success_any();
    let second_server = NativeLineageServer::spawn_retry_success_any();
    attach_pooled_session(
        &fixture,
        &pool,
        &first_server,
        syndic::execution_binding().runtime_id().clone(),
        234_101,
    );
    attach_pooled_session(&fixture, &pool, &second_server, second_runtime, 234_102);

    fixture.store.notify_scheduled_ordinary_execution_ready();
    first_server.wait_for_first_resume();
    second_server.wait_for_first_resume();
    let first_pending = promoted_pending_turn(&fixture.store, &fixture.storage, &first_ids);
    let second_pending = promoted_pending_turn(&fixture.store, &fixture.storage, &second_ids);
    first_server.release_first_resume();
    second_server.release_first_resume();
    first_server.wait_for_initial_retries();
    second_server.wait_for_initial_retries();
    let first_route = wait_for_parked_route_on(&fixture, first_thread);
    let second_route = wait_for_parked_route_on(&fixture, second_thread);
    wait_until("both parked workers released", || {
        (fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0)
            .then_some(())
    });
    assert_eq!(pool.len(), 0);

    let before = fixture.store.accepted_input_scheduler_diagnostics();
    let control = fixture.store.native_lineage_recovery_control();
    control
        .submit(first_route.key(), NativeLineageRecoveryCommand::Retry)
        .unwrap();
    control
        .submit(second_route.key(), NativeLineageRecoveryCommand::Retry)
        .unwrap();
    first_server.wait_for_turn_start();
    second_server.wait_for_turn_start();
    wait_for_turn_complete(&fixture, first_pending);
    wait_for_turn_complete(&fixture, second_pending);
    wait_until("both recovery workers released", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (diagnostics.workers_active() == 0
            && diagnostics.workers_started() >= before.workers_started() + 2)
            .then_some(())
    });
    let after = fixture.store.accepted_input_scheduler_diagnostics();
    assert!(after.coalesced_wake_count() > before.coalesced_wake_count());
    assert!(matches!(
        control.snapshot_for_thread(first_thread).unwrap().status(),
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true,
        }
    ));
    assert!(matches!(
        control.snapshot_for_thread(second_thread).unwrap().status(),
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true,
        }
    ));
    control.acknowledge_leaving(first_route.key()).unwrap();
    control.acknowledge_leaving(second_route.key()).unwrap();

    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    first_server.join();
    second_server.join();
    assert_eq!(pool.len(), 0);
    drop(directory);
}

#[test]
fn native_lineage_command_waits_for_scheduled_worker_capacity_then_runs() {
    let (mut fixture, faults, slot, cas_thread_id) = scheduler_fixture(247, 4);
    let server = NativeLineageServer::spawn_retry_success(cas_thread_id);
    attach_session(&fixture, &slot, &server, 234_103);
    let ids = admit_pending_turn(&mut fixture, &faults, 247);

    server.wait_for_first_resume();
    let pending_turn = promoted_pending_turn(&fixture.store, &fixture.storage, &ids);
    server.release_first_resume();
    server.wait_for_initial_retries();
    let route = wait_for_parked_route(&fixture);
    wait_until("initial worker permit release", || {
        (fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0)
            .then_some(())
    });

    let release_capacity = fixture
        .store
        .saturate_scheduled_ordinary_capacity_for_test();
    let workers_before = fixture
        .store
        .accepted_input_scheduler_diagnostics()
        .workers_started();
    assert_eq!(fixture.store.worker_pool_diagnostics().available(), 0);
    let control = fixture.store.native_lineage_recovery_control();
    control
        .submit(route.key(), NativeLineageRecoveryCommand::Retry)
        .unwrap();
    assert_eq!(
        control
            .snapshot_for_thread(fixture.thread)
            .unwrap()
            .status(),
        NativeLineageRecoveryStatus::Running {
            command: NativeLineageRecoveryCommand::Retry,
        }
    );
    for _ in 0..16_384 {
        std::thread::yield_now();
    }
    assert_eq!(
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_started(),
        workers_before
    );

    release_capacity();
    server.wait_for_turn_start();
    wait_for_turn_complete(&fixture, pending_turn);
    wait_until("capacity-retried worker release", || {
        let scheduler = fixture.store.accepted_input_scheduler_diagnostics();
        (scheduler.workers_started() == workers_before + 1 && scheduler.workers_active() == 0)
            .then_some(())
    });
    assert_eq!(
        control
            .snapshot_for_thread(fixture.thread)
            .unwrap()
            .status(),
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true,
        }
    );
    control.acknowledge_leaving(route.key()).unwrap();

    close_fixture(fixture, &slot, server);
}

#[test]
fn scheduler_parked_cancellation_releases_the_retained_session_without_delivery() {
    let (mut fixture, faults, slot, cas_thread_id) = scheduler_fixture(236, 4);
    let server = NativeLineageServer::spawn_parked(cas_thread_id);
    attach_session(&fixture, &slot, &server, 234_003);
    let ids = admit_pending_turn(&mut fixture, &faults, 236);

    server.wait_for_first_resume();
    let pending_turn = promoted_pending_turn(&fixture.store, &fixture.storage, &ids);
    server.release_first_resume();
    server.wait_for_initial_retries();
    let parked = wait_for_parked_route(&fixture);
    let control = fixture.store.native_lineage_recovery_control();
    control.cancel(parked.key()).unwrap();
    wait_until("cancelled parked session return", || {
        slot.is_ready().then_some(())
    });
    assert!(control.snapshot_for_thread(fixture.thread).is_none());
    assert_eq!(
        control.submit(parked.key(), NativeLineageRecoveryCommand::Retry),
        Err(NativeLineageRecoveryCommandError::Unavailable)
    );
    let command_home = fixture.store.live_home_command().unwrap();
    assert_eq!(
        fixture
            .storage
            .turn_state(command_home.home(), pending_turn, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Pending
    );
    drop(command_home);

    close_fixture(fixture, &slot, server);
}

#[test]
fn service_disposal_drains_a_scheduler_originated_parked_route() {
    let (mut fixture, faults, slot, cas_thread_id) = scheduler_fixture(237, 4);
    let server = NativeLineageServer::spawn_parked(cas_thread_id);
    attach_session(&fixture, &slot, &server, 234_004);
    let _ids = admit_pending_turn(&mut fixture, &faults, 237);

    server.wait_for_first_resume();
    server.release_first_resume();
    server.wait_for_initial_retries();
    let parked = wait_for_parked_route(&fixture);
    let control = fixture.store.native_lineage_recovery_control();
    let thread = fixture.thread;
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    assert!(control.snapshot_for_thread(thread).is_none());
    assert_eq!(
        control.submit(parked.key(), NativeLineageRecoveryCommand::Retry),
        Err(NativeLineageRecoveryCommandError::Unavailable)
    );
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn route_full_candidate_waits_without_churn_while_unrelated_work_progresses() {
    let (mut fixture, faults, pool) = pooled_scheduler_fixture(248, 8);
    let blocked_thread = fixture.thread;
    let (unrelated_thread, unrelated_runtime) =
        prepare_completed_thread(&mut fixture, 249, " unrelated parent");
    let control = fixture.store.native_lineage_recovery_control();
    let mut installed = Vec::new();
    for byte in 180..220 {
        let Some(key) = control.install_route_for_test(
            SyndicThreadId::from_bytes([byte; 16]),
            SyndicThreadId::from_bytes([179; 16]),
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            0,
            true,
        ) else {
            break;
        };
        installed.push(key);
    }
    assert_eq!(installed.len(), 8);
    assert!(
        control
            .install_route_for_test(
                SyndicThreadId::from_bytes([220; 16]),
                SyndicThreadId::from_bytes([179; 16]),
                BindingRevision::new(1).unwrap(),
                NativeLineageOperation::Resume,
                0,
                true,
            )
            .is_none()
    );

    let blocked_server = NativeLineageServer::spawn_retry_success_any();
    attach_pooled_session(
        &fixture,
        &pool,
        &blocked_server,
        syndic::execution_binding().runtime_id().clone(),
        234_104,
    );
    let blocked_ids = seed_pending_turn_on(&mut fixture, &faults, blocked_thread, 248);
    fixture.store.notify_scheduled_ordinary_execution_ready();
    wait_until("pre-dispatch route-capacity denial", || {
        let command_home = fixture.store.live_home_command().ok()?;
        let promoted =
            accepted_route_state(command_home.home(), fixture.storage.clone(), &blocked_ids)
                == AcceptedRouteEffectiveState::Promoted;
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (promoted && diagnostics.workers_active() == 0 && pool.len() == 1).then_some(())
    });
    let blocked_pending = promoted_pending_turn(&fixture.store, &fixture.storage, &blocked_ids);
    let (accepted_before, turn_before) = {
        let command_home = fixture.store.live_home_command().unwrap();
        (
            fixture
                .storage
                .accepted_input(
                    command_home.home(),
                    blocked_ids.accepted_input,
                    point_limit(),
                )
                .unwrap()
                .unwrap(),
            fixture
                .storage
                .turn_state(command_home.home(), blocked_pending, point_limit())
                .unwrap()
                .unwrap(),
        )
    };
    assert!(control.snapshot_for_thread(blocked_thread).is_none());
    assert_eq!(blocked_server.resume_request_count(), 0);
    assert_eq!(pool.len(), 1);
    let command_home = fixture.store.live_home_command().unwrap();
    assert_eq!(
        fixture
            .storage
            .accepted_input(
                command_home.home(),
                blocked_ids.accepted_input,
                point_limit(),
            )
            .unwrap()
            .unwrap(),
        accepted_before
    );
    assert_eq!(
        fixture
            .storage
            .turn_state(command_home.home(), blocked_pending, point_limit())
            .unwrap()
            .unwrap(),
        turn_before
    );
    assert_eq!(
        promoted_pending_turn(&fixture.store, &fixture.storage, &blocked_ids),
        blocked_pending
    );
    drop(command_home);

    let quiet_before = fixture.store.accepted_input_scheduler_diagnostics();
    std::thread::sleep(std::time::Duration::from_millis(50));
    let quiet_after = fixture.store.accepted_input_scheduler_diagnostics();
    assert_eq!(
        quiet_after.workers_started(),
        quiet_before.workers_started()
    );
    assert_eq!(quiet_after.workers_joined(), quiet_before.workers_joined());
    assert_eq!(quiet_after.wake_count(), quiet_before.wake_count());
    assert_eq!(blocked_server.resume_request_count(), 0);

    let unrelated_ids = seed_pending_turn_on(&mut fixture, &faults, unrelated_thread, 249);
    fixture.retire_current_binding(unrelated_thread);
    let unrelated_server = NativeLineageServer::spawn_recovery_success_any();
    attach_pooled_session(
        &fixture,
        &pool,
        &unrelated_server,
        unrelated_runtime,
        234_105,
    );
    fixture.store.notify_scheduled_ordinary_execution_ready();
    unrelated_server.wait_for_turn_start();
    let unrelated_pending = promoted_pending_turn(&fixture.store, &fixture.storage, &unrelated_ids);
    wait_for_turn_complete(&fixture, unrelated_pending);
    wait_until("unrelated worker release", || {
        (fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0)
            .then_some(())
    });
    assert_eq!(blocked_server.resume_request_count(), 0);
    assert!(control.snapshot_for_thread(blocked_thread).is_none());

    let released = installed.remove(0);
    control.cancel(released).unwrap();
    blocked_server.wait_for_first_resume();
    assert_eq!(blocked_server.resume_request_count(), 1);
    blocked_server.release_first_resume();
    blocked_server.wait_for_initial_retries();
    assert_eq!(blocked_server.resume_request_count(), 3);
    let promoted = wait_for_parked_route_on(&fixture, blocked_thread);
    assert_eq!(
        promoted.status(),
        NativeLineageRecoveryStatus::Ready {
            recovery_available: true,
        }
    );
    let command_home = fixture.store.live_home_command().unwrap();
    assert_eq!(
        fixture
            .storage
            .accepted_input(
                command_home.home(),
                blocked_ids.accepted_input,
                point_limit(),
            )
            .unwrap()
            .unwrap(),
        accepted_before
    );
    assert_eq!(
        fixture
            .storage
            .turn_state(command_home.home(), blocked_pending, point_limit())
            .unwrap()
            .unwrap(),
        turn_before
    );
    drop(command_home);

    control
        .submit(promoted.key(), NativeLineageRecoveryCommand::Retry)
        .unwrap();
    blocked_server.wait_for_turn_start();
    assert_eq!(blocked_server.resume_request_count(), 4);
    wait_for_turn_complete(&fixture, blocked_pending);
    wait_until("promoted route retry worker release", || {
        (fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0)
            .then_some(())
    });
    assert_eq!(
        control
            .snapshot_for_thread(blocked_thread)
            .unwrap()
            .status(),
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true,
        }
    );
    control.acknowledge_leaving(promoted.key()).unwrap();
    for key in installed {
        control.cancel(key).unwrap();
    }

    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    blocked_server.join();
    unrelated_server.join();
    assert_eq!(pool.len(), 0);
    drop(directory);
}

#[test]
fn bounded_route_reissues_the_exact_actionable_decision_after_failure() {
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let thread = SyndicThreadId::from_bytes([233; 16]);
    let source = SyndicThreadId::from_bytes([232; 16]);
    let revision = BindingRevision::new(7).unwrap();
    let key = control
        .install_route_for_test(
            thread,
            source,
            revision,
            NativeLineageOperation::Resume,
            3,
            true,
        )
        .unwrap();
    assert!(
        control
            .install_route_for_test(
                SyndicThreadId::from_bytes([231; 16]),
                source,
                revision,
                NativeLineageOperation::Fork,
                3,
                true,
            )
            .is_none()
    );

    let initial = control.snapshot_for_thread(thread).unwrap();
    assert_eq!(initial.source_thread_id(), source);
    assert_eq!(initial.source_binding_revision(), revision);
    assert_eq!(initial.operation(), NativeLineageOperation::Resume);
    assert_eq!(initial.failed_attempts(), 3);
    assert_eq!(
        initial.status(),
        NativeLineageRecoveryStatus::Ready {
            recovery_available: true
        }
    );

    control
        .submit(key, NativeLineageRecoveryCommand::Retry)
        .unwrap();
    assert_eq!(
        control.submit(key, NativeLineageRecoveryCommand::Retry),
        Err(NativeLineageRecoveryCommandError::NotActionable)
    );
    assert_eq!(
        control.take_command_for_test(key),
        Some(NativeLineageRecoveryCommand::Retry)
    );
    assert_eq!(
        control.snapshot_for_thread(thread).unwrap().status(),
        NativeLineageRecoveryStatus::Running {
            command: NativeLineageRecoveryCommand::Retry
        }
    );

    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Failed {
            command: NativeLineageRecoveryCommand::Retry,
            recovery_available: true,
        },
    ));
    let reissued = control.snapshot_for_thread(thread).unwrap();
    assert_eq!(reissued.key(), initial.key());
    assert_eq!(reissued.source_thread_id(), initial.source_thread_id());
    assert_eq!(
        reissued.source_binding_revision(),
        initial.source_binding_revision()
    );
    control
        .submit(key, NativeLineageRecoveryCommand::RecoverFromSyndic)
        .unwrap();
    assert_eq!(
        control.take_command_for_test(key),
        Some(NativeLineageRecoveryCommand::RecoverFromSyndic)
    );

    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true,
        },
    ));
    control.acknowledge_leaving(key).unwrap();
    assert!(control.snapshot_for_thread(thread).is_none());
}

#[test]
fn unavailable_recovery_remains_visible_but_rejects_both_commands() {
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let thread = SyndicThreadId::from_bytes([230; 16]);
    let key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            3,
            false,
        )
        .unwrap();
    assert_eq!(
        control.submit(key, NativeLineageRecoveryCommand::RecoverFromSyndic),
        Err(NativeLineageRecoveryCommandError::NotActionable)
    );
    assert!(control.set_status_for_test(key, NativeLineageRecoveryStatus::Unavailable));
    assert_eq!(
        control.submit(key, NativeLineageRecoveryCommand::Retry),
        Err(NativeLineageRecoveryCommandError::NotActionable)
    );
    assert_eq!(
        control.snapshot_for_thread(thread).unwrap().status(),
        NativeLineageRecoveryStatus::Unavailable
    );
}

#[test]
fn cancellation_releases_capacity_and_stale_route_keys_cannot_target_a_successor() {
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let thread = SyndicThreadId::from_bytes([229; 16]);
    let source = SyndicThreadId::from_bytes([228; 16]);
    let first = control
        .install_route_for_test(
            thread,
            source,
            BindingRevision::new(4).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    control.cancel(first).unwrap();
    assert!(control.snapshot_for_thread(thread).is_none());

    let successor = control
        .install_route_for_test(
            thread,
            source,
            BindingRevision::new(5).unwrap(),
            NativeLineageOperation::Fork,
            2,
            true,
        )
        .unwrap();
    assert_ne!(first, successor);
    assert_eq!(
        control.submit(first, NativeLineageRecoveryCommand::Retry),
        Err(NativeLineageRecoveryCommandError::Stale)
    );
    assert_eq!(
        control.cancel(first),
        Err(NativeLineageRecoveryCommandError::Stale)
    );
    assert_eq!(
        control.snapshot_for_thread(thread).unwrap().key(),
        successor
    );
    control.cancel(successor).unwrap();
    assert!(
        control
            .install_route_for_test(
                SyndicThreadId::from_bytes([227; 16]),
                source,
                BindingRevision::new(6).unwrap(),
                NativeLineageOperation::Resume,
                0,
                false,
            )
            .is_some()
    );
}

#[test]
fn service_disposal_closes_routes_and_rejects_late_commands() {
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let thread = SyndicThreadId::from_bytes([226; 16]);
    let key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            0,
            true,
        )
        .unwrap();
    control.close_for_test();
    assert!(control.snapshot_for_thread(thread).is_none());
    assert_eq!(
        control.submit(key, NativeLineageRecoveryCommand::Retry),
        Err(NativeLineageRecoveryCommandError::Unavailable)
    );
    assert!(
        control
            .install_route_for_test(
                thread,
                thread,
                BindingRevision::new(2).unwrap(),
                NativeLineageOperation::Resume,
                0,
                true,
            )
            .is_none()
    );
}
