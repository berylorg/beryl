use super::*;

use beryl_home_store::test_faults::{FaultController, FaultPoint};

fn run_ambiguous_promotion(
    seed: u8,
    process_generation: u64,
    fault_point: FaultPoint,
    expect_exact: bool,
    worker_capacity: u64,
) {
    let faults = FaultController::new();
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider_faults_and_capacity(
        seed,
        faults.clone(),
        worker_capacity,
        move |assets| Box::new(ready_provider(provider_slot, assets)),
    );
    let parent = fixture.submit_text("phase62 ambiguous promotion parent");
    fixture.complete_with_assistant(parent, "phase62 ambiguous promotion answer");
    let storage = fixture.storage;
    let thread_id = fixture.thread;
    let cas_thread_id = current_cas_thread_id(&fixture.store, storage, thread_id);
    let execution = syndic::execution_binding();
    let server = if expect_exact {
        NormalTerminalServer::spawn_resume_terminal(cas_thread_id)
    } else {
        NormalTerminalServer::spawn_admission_only()
    };
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = fixture
        .store
        .admit(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(process_generation).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    if !expect_exact {
        server.wait_for_admission();
    }

    let barrier = install_scheduled_promotion_barrier(thread_id);
    let ids = admit_runtime_next_input(&mut fixture, seed);
    assert!(
        barrier.wait_until_paused(TIMEOUT),
        "promotion did not reach its reserved pre-command cut"
    );
    if !expect_exact {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        let next_capacity_waits_before = diagnostics.next_capacity_waits();
        let recovered_capacity_waits_before = diagnostics.recovered_pending_capacity_waits();
        fixture.store.notify_scheduled_ordinary_execution_ready();
        wait_until(
            "owned-permit capacity denial in both ordinary lanes",
            || {
                let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
                (diagnostics.next_capacity_waits() > next_capacity_waits_before
                    && diagnostics.recovered_pending_capacity_waits()
                        > recovered_capacity_waits_before)
                    .then_some(())
            },
        );
        assert_eq!(
            fixture
                .store
                .accepted_input_scheduler_diagnostics()
                .next_flight_waits(),
            0,
            "the scheduler armed a flight waiter against its own next worker"
        );
    }
    faults.fail_next(fault_point);
    barrier.release();

    if expect_exact {
        wait_until(
            "ambiguous exact promotion",
            || match try_accepted_route_state(&fixture.store, storage, &ids) {
                Ok(Some(AcceptedRouteEffectiveState::Promoted)) => Some(()),
                Ok(_) | Err(SyndicReadError::Read(beryl_home_store::ReadError::HealthGate(_))) => {
                    None
                }
                Err(error) => panic!("ambiguous promotion read failed: {error}"),
            },
        );
        server.wait_for_projection();
    } else {
        wait_until("ambiguous prior worker completion", || {
            let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
            (diagnostics.workers_joined() >= 1).then_some(())
        });
        thread::sleep(std::time::Duration::from_millis(100));
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        let route = accepted_route_state(&fixture.store, storage, &ids);
        assert!(
            matches!(route, AcceptedRouteEffectiveState::NextTurn(_)),
            "ambiguous Prior must remain parked without cross-lane retry: route={route:?}, diagnostics={diagnostics:?}"
        );
        assert_eq!(diagnostics.workers_started(), 1);
    }
    wait_until("ambiguous promotion session return", || {
        slot.is_ready().then_some(())
    });
    assert!(!fixture.store.accepted_input_scheduler_diagnostics().fatal());

    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn ambiguous_precommit_failure_reconciles_prior_without_cas_or_self_retry() {
    run_ambiguous_promotion(167, 62_004, FaultPoint::BeforeCommit, false, 4);
}

#[test]
fn ambiguous_postcommit_failure_reconciles_exact_before_one_cas_dispatch() {
    run_ambiguous_promotion(168, 62_005, FaultPoint::AfterCommitBeforePersist, true, 128);
}

#[test]
fn known_promotion_conflict_restarts_from_fresh_authority_and_dispatches_once() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(169, move |assets| {
        Box::new(ready_provider(provider_slot, assets))
    });
    let parent = fixture.submit_text("phase62 known-conflict parent");
    fixture.complete_with_assistant(parent, "phase62 known-conflict answer");
    let storage = fixture.storage;
    let thread_id = fixture.thread;
    let cas_thread_id = current_cas_thread_id(&fixture.store, storage, thread_id);
    let execution = syndic::execution_binding();
    let server = NormalTerminalServer::spawn_resume_terminal(cas_thread_id);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = fixture
        .store
        .admit(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(62_006).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);

    let barrier = install_scheduled_promotion_barrier(thread_id);
    let ids = admit_runtime_next_input(&mut fixture, 169);
    assert!(barrier.wait_until_paused(TIMEOUT));
    fixture.advance_unrelated_syndic_revision(240);
    barrier.release();

    wait_until("post-conflict fresh promotion", || {
        (accepted_route_state(&fixture.store, storage, &ids)
            == AcceptedRouteEffectiveState::Promoted)
            .then_some(())
    });
    server.wait_for_projection();
    wait_until("post-conflict session return", || {
        slot.is_ready().then_some(())
    });
    let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
    assert_eq!(diagnostics.workers_started(), 2);
    assert!(!diagnostics.fatal());

    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}
