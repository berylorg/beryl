use super::*;
use crate::cas_projection::connection_work::ConnectionRequestWorkKind;
use crate::cas_projection::runtime_work::RuntimeWorkError;

#[test]
fn router_reads_preserve_observation_and_state_changes_invalidate_it() {
    let router = router(93);
    let boundary = &router.state.boundary;
    let before = boundary.try_observe().unwrap();
    assert_eq!(
        router.try_work_stamp().unwrap(),
        router.work_stamp().unwrap()
    );
    boundary.try_elect(&before, || ()).unwrap();
    let registration = register(&router, "observed-thread", 93, 93, Some("observed-turn"));
    assert!(matches!(
        boundary.try_elect(&before, || ()),
        Err(RuntimeWorkError::Stale)
    ));
    let before_retire = boundary.try_observe().unwrap();
    router.retire(super::super::LiveEventTargetCloseReason::WorkerStopped);
    assert!(matches!(
        boundary.try_elect(&before_retire, || ()),
        Err(RuntimeWorkError::Stale)
    ));
    drop(registration);
}

#[test]
fn response_registration_and_later_release_share_the_router_boundary() {
    let router = router(94);
    let _registration = register(&router, "response-thread", 94, 94, Some("response-turn"));
    let boundary = router.state.boundary.clone();
    let request = approval_request(
        ApprovalRequestKind::Permissions,
        ApprovalResponseDisposition::ResponseRequired,
        None,
        None,
        None,
    );
    let response = request.response_work();
    let before = boundary.try_observe().unwrap();
    router
        .observe_request(
            &mut router.state.lock().unwrap(),
            &CasThreadId::new("response-thread").unwrap(),
            &CasTurnId::new("response-turn").unwrap(),
            ConnectionRequestWorkKind::Approval(ApprovalRequestKind::Permissions),
            response.clone(),
        )
        .unwrap();
    assert!(matches!(
        boundary.try_elect(&before, || ()),
        Err(RuntimeWorkError::Stale)
    ));
    let before_release = boundary.try_observe().unwrap();
    drop(request);
    assert!(matches!(
        boundary.try_elect(&before_release, || ()),
        Err(RuntimeWorkError::Stale)
    ));
    let after = boundary.try_observe().unwrap();
    assert_eq!(response.snapshot().unwrap().retained_capabilities(), 0);
    boundary.try_elect(&after, || ()).unwrap();
    let weak_router = Arc::downgrade(&router);
    drop(router);
    assert!(weak_router.upgrade().is_none());
    assert_eq!(response.snapshot().unwrap().retained_capabilities(), 0);
}

#[test]
fn mutation_interval_ends_before_wait_and_resumes_only_on_mutable_access() {
    let router = router(95);
    let boundary = router.state.boundary.clone();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let worker_router = router.clone();
    let worker = std::thread::spawn(move || {
        let mut state = worker_router.state.lock().unwrap();
        state.quiet_poll_count += 1;
        entered_tx.send(()).unwrap();
        while state.quiet_poll_count == 1 {
            let (next, result) = state
                .wait_timeout(&worker_router.publication_changed, Duration::from_secs(2))
                .unwrap();
            state = next;
            assert!(
                !result.timed_out(),
                "waiter must be released by its test owner"
            );
        }
    });
    entered_rx.recv().unwrap();
    let mut state = router.state.lock().unwrap();
    let observation = boundary.try_observe().unwrap();
    boundary.try_elect(&observation, || ()).unwrap();
    state.quiet_poll_count = 2;
    assert!(matches!(
        boundary.try_observe(),
        Err(RuntimeWorkError::Busy)
    ));
    router.publication_changed.notify_all();
    drop(state);
    worker.join().unwrap();
    assert!(matches!(
        boundary.try_elect(&observation, || ()),
        Err(RuntimeWorkError::Stale)
    ));
}

#[test]
fn queued_request_poll_and_release_invalidate_the_same_boundary() {
    let router = router(97);
    let registration = register(&router, "queue-thread", 97, 97, Some("queue-turn"));
    let request = approval_request(
        ApprovalRequestKind::CommandExecution,
        ApprovalResponseDisposition::ResponseRequired,
        Some(CasThreadId::new("queue-thread").unwrap()),
        Some(CasTurnId::new("queue-turn").unwrap()),
        None,
    );
    assert!(matches!(
        router.route_approval(&live_command(&router), request, None),
        ApprovalRouteOutcome::Routed { .. }
    ));
    let boundary = &router.state.boundary;
    let before_poll = boundary.try_observe().unwrap();
    let LiveEventPoll::Approval(approval) = registration.poll(Duration::ZERO) else {
        panic!("the routed request must be retained until consumed");
    };
    assert!(matches!(
        boundary.try_elect(&before_poll, || ()),
        Err(RuntimeWorkError::Stale)
    ));
    assert_eq!(
        registration
            .queued_operations
            .load(std::sync::atomic::Ordering::Acquire),
        0
    );
    let before_release = boundary.try_observe().unwrap();
    drop(approval);
    assert!(matches!(
        boundary.try_elect(&before_release, || ()),
        Err(RuntimeWorkError::Stale)
    ));
}

#[test]
fn queued_request_settles_after_router_poison_without_reopening_observation() {
    let router = router(99);
    let registration = register(&router, "queue-thread", 99, 99, Some("queue-turn"));
    let request = approval_request(
        ApprovalRequestKind::CommandExecution,
        ApprovalResponseDisposition::ResponseRequired,
        Some(CasThreadId::new("queue-thread").unwrap()),
        Some(CasTurnId::new("queue-turn").unwrap()),
        None,
    );
    assert!(matches!(
        router.route_approval(&live_command(&router), request, None),
        ApprovalRouteOutcome::Routed { .. }
    ));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = router.state.lock().unwrap();
            panic!("poison the router before its consumer settles");
        }))
        .is_err()
    );
    let LiveEventPoll::Approval(approval) = registration.poll(Duration::ZERO) else {
        panic!("the retained consumer must still release its queued operation");
    };
    assert_eq!(
        registration
            .queued_operations
            .load(std::sync::atomic::Ordering::Acquire),
        0
    );
    drop(approval);
    assert!(matches!(
        router.state.boundary.try_observe(),
        Err(RuntimeWorkError::Unavailable)
    ));
}

#[test]
fn exact_connection_election_defers_registered_response_release_until_publication() {
    let router = router(98);
    let _registration = register(&router, "response-thread", 98, 98, Some("response-turn"));
    let request = approval_request(
        ApprovalRequestKind::Permissions,
        ApprovalResponseDisposition::ResponseRequired,
        None,
        None,
        None,
    );
    let response = request.response_work();
    router
        .observe_request(
            &mut router.state.lock().unwrap(),
            &CasThreadId::new("response-thread").unwrap(),
            &CasTurnId::new("response-turn").unwrap(),
            ConnectionRequestWorkKind::Approval(ApprovalRequestKind::Permissions),
            response.clone(),
        )
        .unwrap();
    let boundary = &router.state.boundary;
    let observation = boundary.try_observe().unwrap();
    let (start_tx, start_rx) = std::sync::mpsc::channel();
    let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        start_rx.recv().unwrap();
        attempt_tx.send(()).unwrap();
        drop(request);
        done_tx.send(()).unwrap();
    });
    let held = boundary
        .try_elect(&observation, || {
            start_tx.send(()).unwrap();
            attempt_rx.recv().unwrap();
            done_rx.recv_timeout(Duration::from_millis(50))
        })
        .unwrap();
    worker.join().unwrap();
    assert!(matches!(
        held,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    assert_eq!(response.snapshot().unwrap().retained_capabilities(), 0);
    assert!(matches!(
        boundary.try_elect(&observation, || ()),
        Err(RuntimeWorkError::Stale)
    ));
}

#[test]
fn router_guard_unwind_invalidates_boundary_and_retirement_recovers_poison() {
    let router = router(96);
    let before = router.state.boundary.try_observe().unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = router.state.lock().unwrap();
            panic!("interrupt router access");
        }))
        .is_err()
    );
    assert!(matches!(
        router.state.boundary.try_elect(&before, || ()),
        Err(RuntimeWorkError::Unavailable)
    ));
    router.retire(super::super::LiveEventTargetCloseReason::WorkerStopped);
    assert!(
        router
            .state
            .lock()
            .unwrap_err()
            .into_inner()
            .retired
            .is_some()
    );
}
