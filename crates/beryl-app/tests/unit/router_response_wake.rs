use super::*;
use crate::cas_projection::connection_work::ConnectionRequestWorkKind;

fn notifications(router: &EventRouter) -> u64 {
    let diagnostics = router.scheduler_signal.diagnostics();
    diagnostics.wake_count() + diagnostics.coalesced_wake_count()
}

#[test]
fn observing_a_written_response_wakes_while_its_capability_and_record_remain() {
    let router = router(90);
    let _registration = register(&router, "response-thread", 90, 90, Some("response-turn"));
    let request = approval_request(
        ApprovalRequestKind::CommandExecution,
        ApprovalResponseDisposition::AutoDenied,
        None,
        None,
        None,
    );
    let observer = request.response_work();
    let before = notifications(&router);
    let serial = router
        .observe_request(
            &mut router.state.lock().unwrap(),
            &CasThreadId::new("response-thread").unwrap(),
            &CasTurnId::new("response-turn").unwrap(),
            ConnectionRequestWorkKind::Approval(ApprovalRequestKind::CommandExecution),
            observer.clone(),
        )
        .unwrap();
    assert_eq!(notifications(&router), before + 1);
    let snapshot = observer.snapshot().unwrap();
    assert!(snapshot.response_written());
    assert_eq!(snapshot.retained_capabilities(), 1);
    assert!(
        router
            .state
            .lock()
            .unwrap()
            .work_requests
            .contains_key(&serial)
    );
    drop(request);
    assert_eq!(notifications(&router), before + 1);
}

#[test]
fn final_unwritten_response_release_wakes_with_its_observation_still_retained() {
    let router = router(91);
    let _registration = register(&router, "response-thread", 91, 91, Some("response-turn"));
    let request = approval_request(
        ApprovalRequestKind::Permissions,
        ApprovalResponseDisposition::ResponseRequired,
        None,
        None,
        None,
    );
    let observer = request.response_work();
    let before = notifications(&router);
    let serial = router
        .observe_request(
            &mut router.state.lock().unwrap(),
            &CasThreadId::new("response-thread").unwrap(),
            &CasTurnId::new("response-turn").unwrap(),
            ConnectionRequestWorkKind::Approval(ApprovalRequestKind::Permissions),
            observer.clone(),
        )
        .unwrap();
    assert_eq!(notifications(&router), before);
    drop(request);
    assert_eq!(notifications(&router), before + 1);
    let snapshot = observer.snapshot().unwrap();
    assert!(!snapshot.response_written());
    assert_eq!(snapshot.retained_capabilities(), 0);
    assert!(
        router
            .state
            .lock()
            .unwrap()
            .work_requests
            .contains_key(&serial)
    );
}
