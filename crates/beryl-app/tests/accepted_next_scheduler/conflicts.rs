use super::*;

use beryl_app::cas_projection::test_faults::install_scheduled_promotion_reconciliation_barrier;
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};

fn assert_candidate_unpromoted(fixture: &syndic::Fixture, ids: &support::NextRecordIds) {
    let command_home = fixture.store.live_home_command().unwrap();
    let home = command_home.home();
    assert_eq!(
        accepted_route_state(home, &fixture.storage, ids),
        AcceptedRouteEffectiveState::NextTurn(syndic_storage::NextTurnReason::UnknownTerminal)
    );
    assert_eq!(
        fixture
            .storage
            .thread(home, ids.thread, support::point_limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(ids.parent)
    );
    assert!(!fixture.store.accepted_input_scheduler_diagnostics().fatal());
}

#[test]
fn repeated_unrelated_commit_conflicts_rescan_and_dispatch_once_without_another_wake() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(180, move |assets| {
        Box::new(ready_provider(provider_slot, assets))
    });
    let parent = fixture.submit_text(" conflict parent");
    fixture.complete_with_assistant(parent, " conflict answer");
    let cas_thread_id = {
        let command_home = fixture.store.live_home_command().unwrap();
        current_cas_thread_id(command_home.home(), &fixture.storage, fixture.thread)
    };
    let server = NormalTerminalServer::spawn_resume_terminal(cas_thread_id);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            syndic::execution_binding().runtime_id(),
            CasProcessGeneration::new(62_020).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);

    let first_reserved = install_scheduled_promotion_barrier(fixture.thread);
    let first_outcome = install_scheduled_promotion_reconciliation_barrier(fixture.thread);
    let ids = admit_runtime_next_input(&mut fixture, 180);
    assert!(first_reserved.wait_until_paused(TIMEOUT));
    let first_revision = {
        let command_home = fixture.store.live_home_command().unwrap();
        command_home.home().home_revision().unwrap()
    };
    fixture.advance_unrelated_syndic_revision(182);
    drop(first_reserved);
    assert!(first_outcome.wait_until_paused(TIMEOUT));
    assert_candidate_unpromoted(&fixture, &ids);
    {
        let command_home = fixture.store.live_home_command().unwrap();
        assert_ne!(command_home.home().home_revision().unwrap(), first_revision);
    }

    let second_reserved = install_scheduled_promotion_barrier(fixture.thread);
    drop(first_outcome);
    assert!(second_reserved.wait_until_paused(TIMEOUT));
    let second_outcome = install_scheduled_promotion_reconciliation_barrier(fixture.thread);
    fixture.advance_unrelated_syndic_revision(184);
    drop(second_reserved);
    assert!(second_outcome.wait_until_paused(TIMEOUT));
    assert_candidate_unpromoted(&fixture, &ids);
    drop(second_outcome);

    server.wait_for_projection();
    let successor = wait_until("terminal after repeated promotion conflicts", || {
        let command_home = fixture.store.live_home_command().ok()?;
        let home = command_home.home();
        let successor = fixture
            .storage
            .thread(home, ids.thread, support::point_limit())
            .ok()
            .flatten()?
            .committed_tail()?;
        let state = fixture
            .storage
            .turn_state(home, successor, support::point_limit())
            .ok()
            .flatten()?;
        (state.lifecycle() == TurnLifecycle::Complete && successor != ids.parent)
            .then_some(successor)
    });
    assert_ne!(successor, ids.parent);
    {
        let command_home = fixture.store.live_home_command().unwrap();
        assert_eq!(
            accepted_route_state(command_home.home(), &fixture.storage, &ids),
            AcceptedRouteEffectiveState::Promoted
        );
    }
    wait_until("conflict retry session return and scan release", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (slot.is_ready()
            && diagnostics.workers_active() == 0
            && !diagnostics.next_retained_source_cursor()
            && !diagnostics.next_retained_candidate_cursor())
        .then_some(())
    });
    assert!(!fixture.store.accepted_input_scheduler_diagnostics().fatal());
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn service_shutdown_joins_uncommitted_conflict_and_preserves_accepted_input() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(186, move |assets| {
        Box::new(ready_provider(provider_slot, assets))
    });
    let parent = fixture.submit_text(" conflict shutdown parent");
    fixture.complete_with_assistant(parent, " conflict shutdown answer");
    let server = NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            syndic::execution_binding().runtime_id(),
            CasProcessGeneration::new(62_021).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    slot.replace(session);
    server.wait_for_admission();

    let reserved = install_scheduled_promotion_barrier(fixture.thread);
    let outcome = install_scheduled_promotion_reconciliation_barrier(fixture.thread);
    let ids = admit_runtime_next_input(&mut fixture, 186);
    assert!(reserved.wait_until_paused(TIMEOUT));
    fixture.advance_unrelated_syndic_revision(188);
    drop(reserved);
    assert!(outcome.wait_until_paused(TIMEOUT));
    assert_candidate_unpromoted(&fixture, &ids);

    let (directory, service) = fixture.into_service();
    let close_worker = thread::spawn(move || service.close());
    wait_until("conflicted promotion shutdown fence", || {
        retirement.is_retired().then_some(())
    });
    assert!(!close_worker.is_finished());
    drop(outcome);
    drop(retirement);
    assert!(matches!(
        close_worker.join().unwrap().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert!(!slot.is_ready());

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        accepted_route_state(&reopened, &storage, &ids),
        AcceptedRouteEffectiveState::NextTurn(syndic_storage::NextTurnReason::UnknownTerminal)
    );
    assert_eq!(
        storage
            .thread(&reopened, ids.thread, support::point_limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(ids.parent)
    );
    reopened.close().unwrap();
    drop(directory);
}
