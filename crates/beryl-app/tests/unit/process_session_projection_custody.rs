use super::*;
use crate::cas_projection::{CasProjectionCoordinator, LoadedCasProjection};
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_model::{BindingRevision, CasThreadId, ThreadRevision};
use syndic_storage::{
    CasLineageProof, CasRepresentedPrefixProof, NativeCasLineage, empty_selected_path_digest,
};

#[test]
fn admitted_shutdown_preserves_foreign_or_stale_fences_and_checked_out_or_shared_authority() {
    let (_directory, service, state, sessions) = owned_service(6);
    let (session, server) = admitted_session(&service, 72_182);
    let connection = Arc::clone(session.connection());
    let thread = SyndicThreadId::from_bytes([182; 16]);
    let cas = CasThreadId::new("shutdown-owned-projection").unwrap();
    let loaded = connection
        .register_new(cas.clone(), Default::default(), thread, TIMEOUT)
        .unwrap();
    let projection = {
        let live = service.live_home_command().unwrap();
        let coordinator = CasProjectionCoordinator::for_healthy_home(live.home()).unwrap();
        LoadedCasProjection::new(
            &coordinator,
            thread,
            BindingRevision::new(1).unwrap(),
            execution_binding(RuntimeId::from_bytes([91; 16])),
            cas.clone(),
            loaded,
            CasLineageProof::native(
                NativeCasLineage::Fresh,
                CasRepresentedPrefixProof::new(
                    None,
                    ThreadRevision::new(1).unwrap(),
                    empty_selected_path_digest(),
                ),
            )
            .unwrap(),
        )
    };
    let registration = sessions
        .register(
            thread,
            projection.execution_binding().clone(),
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        )
        .unwrap();
    let mut checkout = issued(&service, thread);
    checkout.return_projection(projection);
    let ScheduledOrdinaryExecutionLease {
        session: checked_out_session,
        tools: checked_out_tools,
        _worker,
        flight,
        ..
    } = checkout;
    drop(flight);
    drop(_worker);
    let commands = service.live_command_authorizer();
    let foreign = crate::process_admission::ProcessAdmissionGate::new()
        .fence()
        .unwrap();
    sessions.retire_idle_for_shutdown(&foreign).unwrap();
    assert_eq!(sessions.diagnostics().checked_out, 1);
    let fence = commands.prepare_process_closing().unwrap().publish();
    sessions.retire_idle_for_shutdown(&fence).unwrap();
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert!(!connection.is_retired());
    drop(checked_out_session);
    drop(checked_out_tools);
    sessions.retire_idle_for_shutdown(&foreign).unwrap();
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(!connection.settlement_cleanup_complete(thread).unwrap());
    let authority = connection.lock_authority_for_test();
    sessions.retire_idle_for_shutdown(&fence).unwrap();
    assert_eq!(sessions.diagnostics().available, 1);
    drop(authority);
    let promotion = connection.reserve_scheduled_promotion().unwrap().unwrap();
    sessions.retire_idle_for_shutdown(&fence).unwrap();
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(!connection.settlement_cleanup_complete(thread).unwrap());
    drop(promotion);
    let cleanup = connection.acquire_cleanup_owner().unwrap().unwrap();
    sessions.retire_idle_for_shutdown(&fence).unwrap();
    assert_eq!(sessions.diagnostics().available, 1);
    drop(cleanup);
    let crate::cas_projection::connection::ExistingLease::Exact(external) =
        connection.acquire_existing(&cas, thread, TIMEOUT).unwrap()
    else {
        panic!("the same original target must remain loaded");
    };
    sessions.retire_idle_for_shutdown(&fence).unwrap();
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(external.is_live().unwrap());
    drop(external);
    fence.reopen_if(true).unwrap();
    sessions.retire_idle_for_shutdown(&fence).unwrap();
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(!connection.settlement_cleanup_complete(thread).unwrap());
    assert!(!connection.is_retired());
    assert!(sessions.retire(registration));
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
}

#[test]
fn a_failure_between_projection_return_and_checkout_drop_settles_only_the_original_lease_locally() {
    let faults = FaultController::new();
    let (_directory, service, state, sessions) = owned_service_with_faults(6, Some(faults.clone()));
    let (session, server) = admitted_session(&service, 72_180);
    let thread = SyndicThreadId::from_bytes([180; 16]);
    let connection = Arc::clone(session.connection());
    let loaded = connection
        .register_new(
            CasThreadId::new("late-checkout-return").unwrap(),
            Default::default(),
            thread,
            TIMEOUT,
        )
        .unwrap();
    let external_thread = SyndicThreadId::from_bytes([181; 16]);
    let external = connection
        .register_new(
            CasThreadId::new("external-loaded-lease").unwrap(),
            Default::default(),
            external_thread,
            TIMEOUT,
        )
        .unwrap();
    let live = service.live_home_command().unwrap();
    let home = live.home();
    let storage = SyndicStorage::reacquire(home).unwrap();
    let commands = service.live_command_authorizer();
    let coordinator = CasProjectionCoordinator::for_healthy_home(home).unwrap();
    let represented = CasRepresentedPrefixProof::new(
        None,
        ThreadRevision::new(1).unwrap(),
        empty_selected_path_digest(),
    );
    let projection = LoadedCasProjection::new(
        &coordinator,
        thread,
        BindingRevision::new(1).unwrap(),
        execution_binding(RuntimeId::from_bytes([91; 16])),
        CasThreadId::new("late-checkout-return").unwrap(),
        loaded,
        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
    );
    sessions
        .register(
            thread,
            projection.execution_binding().clone(),
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        )
        .unwrap();
    let mut lease = issued(&service, thread);
    lease.return_projection(projection);
    assert!(!connection.settlement_cleanup_complete(thread).unwrap());
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        storage
            .thread(
                home,
                thread,
                syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()
            )
            .is_err()
    );
    commands.observe_persistent_failure();
    let deadline = Instant::now() + TIMEOUT;
    while !commands.is_persistent_failure_cut() {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    drop(lease);
    assert!(connection.settlement_cleanup_complete(thread).unwrap());
    assert!(external.is_live().unwrap());
    assert_eq!(sessions.diagnostics().available, 0);
    assert!(!connection.is_retired());
    drop(external);
    drop(live);
    match service.close() {
        Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(_)) => {}
        Err(failure) => {
            assert!(matches!(
                failure.error(),
                crate::cas_projection::ProjectionConnectionServiceCloseError::SchedulerShutdown
            ));
            drop(failure);
        }
        other => panic!("failed Home must retain its failure disposition: {other:?}"),
    }
    server.join();
}
