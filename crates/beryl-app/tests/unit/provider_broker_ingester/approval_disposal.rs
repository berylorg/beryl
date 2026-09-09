use super::*;
use crate::cas_projection::connection::{
    registry::LoadedThreadKey,
    router::{ApprovalRouteOutcome, PreparedApprovalInterruption, TargetTurnRegistration},
};
use beryl_backend::{
    ApprovalInterruption, ApprovalRequestKind, ApprovalResponseDisposition, ExactForegroundTurn,
    StopAttemptCorrelation, StopAttemptDisposition, StopOperationCorrelation,
    lifecycle_test_support::approval_request,
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasThreadId, CasTurnId, SyndicThreadId,
};

fn verify_failed_ingester_slot_disposal(pending: bool) {
    let fixture = BrokerBuildFixture::new(if pending { 202 } else { 203 });
    let PreparedProviderBroker {
        sink,
        control,
        ingester,
        start,
    } = fixture.prepare_start_blocked_with_fault(ProviderBrokerBuildFault::IngesterPanic);
    let slot = Arc::clone(&control.approval);
    assert!(slot.reserve());
    let registration = if pending {
        let thread_id = CasThreadId::new("permission-panic-thread").unwrap();
        let turn_id = CasTurnId::new("permission-panic-turn").unwrap();
        let runtime_id = RuntimeId::from_bytes([202; 16]);
        let process_generation = CasProcessGeneration::new(82_202).unwrap();
        let loaded = CasLoadedSessionGeneration::new(
            process_generation,
            CasLoadedThreadGeneration::new(1).unwrap(),
        );
        let command = fixture.commands.authorize().unwrap();
        let registration = fixture
            .router
            .register(
                &command,
                LoadedThreadKey {
                    runtime_id,
                    process_generation,
                    cas_thread_id: thread_id.clone(),
                },
                SyndicThreadId::from_bytes([202; 16]),
                loaded,
                fixture.home_generation.get(),
                Duration::from_secs(1),
                TargetTurnRegistration::Active(turn_id.clone()),
            )
            .unwrap();
        let request = approval_request(
            ApprovalRequestKind::Permissions,
            ApprovalResponseDisposition::ResponseRequired,
            Some(thread_id.clone()),
            Some(turn_id.clone()),
            None,
        );
        let interruption = ApprovalInterruption::DurableStopOwned {
            operation: StopOperationCorrelation::from_bytes([202; 16]),
            target: ExactForegroundTurn::new(runtime_id, loaded, thread_id, turn_id),
            attempt_disposition: StopAttemptDisposition::PossiblyDispatched(
                StopAttemptCorrelation::from_bytes([203; 16]),
            ),
        };
        let outcome = fixture.router.route_approval(
            &command,
            request,
            Some(PreparedApprovalInterruption::new(interruption, None)),
        );
        let ApprovalRouteOutcome::Routed {
            obligation: Some(mut obligation),
            ..
        } = outcome
        else {
            panic!("exact joined permission did not create its obligation");
        };
        assert!(obligation.take_primary().is_none());
        assert!(slot.install(obligation));
        Some(registration)
    } else {
        None
    };
    assert!(!slot.is_closed_for_test());
    assert_eq!(fixture.workers.diagnostics().active(), 1);

    let running = ingester.start(start);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !running.is_finished() {
        assert!(Instant::now() < deadline, "failed ingester did not exit");
        thread::yield_now();
    }
    assert!(!control.cancelled.load(Ordering::Acquire));
    assert!(
        slot.is_closed_for_test(),
        "completion must dispose the slot before later cancellation"
    );
    assert_eq!(fixture.workers.diagnostics().active(), 1);

    assert!(fixture.workers.try_acquire_pair().is_err());
    running.arm_ordinary_worker_release().unwrap();
    assert_eq!(fixture.workers.diagnostics().active(), 0);
    assert!(slot.is_closed_for_test());
    let replacement = fixture.workers.try_acquire_pair().unwrap();
    assert!(slot.take().is_none());
    assert!(!slot.reserve());
    drop(replacement);

    let stopped = running.stop_and_join();
    assert!(!stopped.receipt().clean);
    assert_eq!(fixture.workers.diagnostics().active(), 0);
    drop(registration);
    drop(sink);
    drop(control);
}

#[test]
fn failed_ingester_disposes_joined_pending_permission_before_worker_release() {
    verify_failed_ingester_slot_disposal(true);
}

#[test]
fn failed_ingester_closes_reserved_permission_before_worker_release() {
    verify_failed_ingester_slot_disposal(false);
}
