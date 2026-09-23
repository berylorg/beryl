use super::*;
use crate::cas_projection::test_faults::OutageCaptureState;

#[test]
fn oversized_active_inventory_disables_capture_without_blocking_failure_cut() {
    use crate::cas_projection::connection::{LoadedThreadKey, TargetTurnRegistration};
    use beryl_model::{
        CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasThreadId, CasTurnId,
        SyndicThreadId, SyndicTurnId,
    };
    let mut config =
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap();
    config.outage_buffer.max_targets = 0;
    let (_directory, faults, state, _, service) = service_with_config(config);
    let (server, session) = admitted_connection(&service, 102_160);
    let router = session.connection().original_failure_router_for_test();
    let command = service.command_authorizer.authorize().unwrap();
    let process = CasProcessGeneration::new(102_160).unwrap();
    let registration = router
        .register(
            &command,
            LoadedThreadKey {
                runtime_id: RuntimeId::from_bytes([102; 16]),
                process_generation: process,
                cas_thread_id: CasThreadId::new("active-over-limit").unwrap(),
            },
            SyndicThreadId::from_bytes([160; 16]),
            CasLoadedSessionGeneration::new(process, CasLoadedThreadGeneration::new(1).unwrap()),
            service.home_generation.get(),
            Duration::from_secs(1),
            TargetTurnRegistration::Active {
                syndic_turn_id: SyndicTurnId::from_bytes([161; 16]),
                cas_turn_id: CasTurnId::new("active-turn").unwrap(),
            },
        )
        .unwrap();
    drop(command);
    fail_home(&service, state, &faults);
    wait_until("oversized inventory cut", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    assert_eq!(
        service.outage_inventory.snapshot().state,
        OutageCaptureState::Unavailable
    );
    assert_eq!(service.outage_inventory.snapshot().targets, 0);
    drop(registration);
    let _ = service.close().unwrap();
    drop(session);
    drop(router);
    server.join();
}

#[test]
fn invalid_registry_before_failure_capture_never_installs_obligations() {
    for poison in [false, true] {
        let (_directory, faults, state, _, service) = service();
        let (server, session) = admitted_connection(&service, 102_150);
        let connection = Arc::clone(session.connection());
        if poison {
            service.connections.poison_for_test();
        } else {
            service.connections.exhaust_revision_for_test();
        }
        fail_home(&service, state, &faults);
        wait_until("invalid capture ownership", || {
            service.persistent_failure_cut_snapshot().state()
                == PersistentFailureCutState::Incomplete
        });
        assert!(!connection.failure_obligations_installed_for_test());
        assert_eq!(
            service.outage_inventory.snapshot().state,
            OutageCaptureState::Unavailable
        );
        let _ = service.close();
        drop(session);
        drop(connection);
        server.join();
    }
}

#[test]
fn failure_cut_waits_for_prepared_connection_publication_before_traversal() {
    use crate::cas_projection::test_faults::{
        AcquisitionBarrierStage, install_acquisition_barrier,
    };
    let (_directory, faults, state, _, service) = service();
    let server = terminal_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        terminal_server::AUTHORIZATION,
    );
    let barrier = install_acquisition_barrier(
        service.service_generation(),
        AcquisitionBarrierStage::SessionPrepared,
    );
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            service.admit_lifecycle_test_candidate(
                &connector,
                RuntimeId::from_bytes([102; 16]),
                CasProcessGeneration::new(102_140).unwrap(),
                Path::new(r"C:\work\beryl"),
                terminal_server::TIMEOUT,
            )
        });
        barrier.wait();
        assert_eq!(service.connections.lock().unwrap().len(), 0);
        fail_home(&service, state, &faults);
        wait_until("the cut waiting on the prepared connection", || {
            service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Cutting
        });
        barrier.release();
        assert!(worker.join().unwrap().is_err());
    });
    server.wait_for_admission();
    wait_until("the published late connection to be captured", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    assert_eq!(service.connections.lock().unwrap().len(), 1);
    assert_eq!(service.worker_pool_diagnostics().active(), 0);
    let _ = service.close().unwrap();
    server.join();
}

#[test]
fn historical_failed_connections_beyond_capacity_do_not_enter_retained_capture() {
    let (_directory, faults, state, _, service) = service();
    let mut historical = Vec::new();
    let mut historical_routers = Vec::new();
    for generation in 102_100..102_109 {
        let (server, session) = admitted_connection(&service, generation);
        let connection = Arc::clone(session.connection());
        historical_routers.push(connection.original_failure_router_for_test());
        connection.fail_next_ingester_join_for_test();
        drop(session);
        assert!(connection.shutdown().is_err());
        assert!(connection.retain_persistent_failure_workers().is_none());
        connection.poison_forwarding_hub_for_test();
        assert!(!connection.is_detached());
        historical.push(Arc::downgrade(&connection));
        drop(connection);
        server.join();
    }
    let (server, session) = admitted_connection(&service, 102_110);
    let live = Arc::clone(session.connection());
    let live_router = live.original_failure_router_for_test();
    let baseline = historical
        .iter()
        .map(std::sync::Weak::strong_count)
        .collect::<Vec<_>>();
    std::thread::scope(|scope| {
        let (reached, waiting) = std::sync::mpsc::sync_channel(1);
        let (release, released) = std::sync::mpsc::sync_channel(1);
        let locked_router = &live_router;
        let lock = scope.spawn(move || {
            locked_router.with_failure_state_locked_for_test(|| {
                reached.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(5)).unwrap();
            })
        });
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        fail_home(&service, state, &faults);
        wait_until("the historical capture prefix", || {
            Arc::strong_count(&live) > 3
        });
        for (weak, expected) in historical.iter().zip(baseline) {
            assert_eq!(weak.strong_count(), expected);
        }
        assert!(
            historical_routers
                .iter()
                .all(|router| router.failure_is_frozen_for_test())
        );
        assert!(!live.failure_obligations_installed_for_test());
        release.send(()).unwrap();
        lock.join().unwrap();
    });
    wait_until("the bounded historical cut", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    assert_eq!(service.connections.lock().unwrap().len(), 10);
    assert!(matches!(
        service.close(),
        Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
    ));
    drop(session);
    drop(live);
    drop(live_router);
    server.join();
}

#[test]
fn complete_freeze_and_final_membership_validation_precede_obligation_installation() {
    for mutate_membership in [false, true] {
        let (_directory, faults, state, _, service) = service_with_worker_capacity(8);
        let (first_server, first_session) = admitted_connection(&service, 102_120);
        let (second_server, second_session) = admitted_connection(&service, 102_121);
        let first = Arc::clone(first_session.connection());
        let second = Arc::clone(second_session.connection());
        let first_router = first.original_failure_router_for_test();
        let second_router = second.original_failure_router_for_test();
        std::thread::scope(|scope| {
            let (reached, waiting) = std::sync::mpsc::sync_channel(1);
            let (release, released) = std::sync::mpsc::sync_channel(1);
            let locked_router = &second_router;
            let lock = scope.spawn(move || {
                locked_router.with_failure_state_locked_for_test(|| {
                    reached.send(()).unwrap();
                    released.recv_timeout(Duration::from_secs(5)).unwrap();
                })
            });
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            fail_home(&service, state, &faults);
            wait_until("the first frozen router", || {
                first_router.failure_is_frozen_for_test()
            });
            assert!(!first.failure_obligations_installed_for_test());
            assert!(!second.failure_obligations_installed_for_test());
            assert_eq!(
                service.outage_inventory.snapshot().state,
                OutageCaptureState::Pending
            );
            if mutate_membership {
                service.connections.lock().unwrap().reverse();
            }
            release.send(()).unwrap();
            lock.join().unwrap();
        });
        let expected = if mutate_membership {
            PersistentFailureCutState::Incomplete
        } else {
            PersistentFailureCutState::Finished
        };
        wait_until("the exact membership outcome", || {
            service.persistent_failure_cut_snapshot().state() == expected
        });
        assert_eq!(
            first.failure_obligations_installed_for_test(),
            !mutate_membership
        );
        assert_eq!(
            second.failure_obligations_installed_for_test(),
            !mutate_membership
        );
        let retained_inventory = Arc::clone(&service.outage_inventory);
        assert_eq!(
            retained_inventory.snapshot().state,
            if mutate_membership {
                OutageCaptureState::Unavailable
            } else {
                OutageCaptureState::Ready
            }
        );
        let _ = service.close();
        assert_eq!(
            retained_inventory.snapshot().state,
            OutageCaptureState::Unavailable
        );
        assert_eq!(retained_inventory.snapshot().facts, 0);
        drop(first_session);
        drop(second_session);
        drop(first);
        drop(second);
        drop(first_router);
        drop(second_router);
        first_server.join();
        second_server.join();
    }
}

#[test]
fn original_router_remains_reachable_through_hub_poison_but_router_poison_prevents_installation() {
    for poison_router in [false, true] {
        let (_directory, faults, state, _, service) = service();
        let (server, session) = admitted_connection(&service, 102_130);
        let connection = Arc::clone(session.connection());
        let router = connection.original_failure_router_for_test();
        if poison_router {
            router.poison_failure_state_for_test();
        } else {
            connection.poison_forwarding_hub_for_test();
        }
        fail_home(&service, state, &faults);
        let expected = if poison_router {
            PersistentFailureCutState::Incomplete
        } else {
            PersistentFailureCutState::Finished
        };
        wait_until("the original router health outcome", || {
            service.persistent_failure_cut_snapshot().state() == expected
        });
        if poison_router {
            assert!(!connection.failure_obligations_installed_for_test());
        } else {
            assert!(router.failure_is_frozen_for_test());
        }
        let _ = service.close();
        drop(session);
        drop(connection);
        drop(router);
        server.join();
    }
}
