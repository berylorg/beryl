use super::*;

#[test]
fn idle_retirement_preserves_promotion_and_cleanup_owners_and_later_checkout() {
    let (directory, service, state, sessions) = owned_service(6);
    let (session, server) = admitted_session(&service, 72_101);
    let connection = Arc::clone(session.connection());
    let thread = SyndicThreadId::from_bytes([211; 16]);
    let registration = sessions
        .register(
            thread,
            execution_binding(RuntimeId::from_bytes([91; 16])),
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        )
        .unwrap();

    let promotion = connection.reserve_scheduled_promotion().unwrap().unwrap();
    assert!(!sessions.retire_if_idle(registration).unwrap());
    assert_eq!(sessions.diagnostics().available, 1);
    drop(promotion);
    assert!(!connection.is_retired());
    let cleanup = connection.acquire_cleanup_owner().unwrap().unwrap();
    assert!(!sessions.retire_if_idle(registration).unwrap());
    drop(cleanup);
    assert!(!connection.is_retired());

    let checkout = issued(&service, thread);
    assert!(!sessions.retire_if_idle(registration).unwrap());
    assert_eq!(sessions.diagnostics().checked_out, 1);
    drop(checkout);
    assert!(sessions.retire_if_idle(registration).unwrap());
    assert!(connection.is_retired());
    assert!(matches!(
        issue(
            &service,
            thread,
            execution_binding(RuntimeId::from_bytes([91; 16]))
        )
        .unwrap(),
        ScheduledOrdinaryAdmissionResult::Unavailable(_)
    ));
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
}

#[test]
fn idle_retirement_and_checkout_have_one_winner() {
    let (directory, service, state, sessions) = owned_service(6);
    let (session, server) = admitted_session(&service, 72_102);
    let thread = SyndicThreadId::from_bytes([212; 16]);
    let binding = execution_binding(RuntimeId::from_bytes([91; 16]));
    let registration = sessions
        .register(
            thread,
            binding.clone(),
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        )
        .unwrap();
    let barrier = std::sync::Barrier::new(2);
    let (retired, checkout) = std::thread::scope(|scope| {
        let retiring = scope.spawn(|| {
            barrier.wait();
            sessions.retire_if_idle(registration).unwrap()
        });
        barrier.wait();
        let checkout = issue(&service, thread, binding).unwrap();
        (retiring.join().unwrap(), checkout)
    });
    match checkout {
        ScheduledOrdinaryAdmissionResult::Issued(lease) => {
            assert!(!retired);
            assert_eq!(sessions.diagnostics().checked_out, 1);
            drop(lease);
            assert!(sessions.retire_if_idle(registration).unwrap());
        }
        ScheduledOrdinaryAdmissionResult::Unavailable(_) => assert!(retired),
    }
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
}

#[test]
fn stale_idle_retirement_cannot_remove_a_replacement_session() {
    let (directory, service, state, sessions) = owned_service(6);
    let thread = SyndicThreadId::from_bytes([213; 16]);
    let binding = execution_binding(RuntimeId::from_bytes([91; 16]));
    let (session, first_server) = admitted_session(&service, 72_103);
    let old = sessions
        .register(
            thread,
            binding.clone(),
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        )
        .unwrap();
    assert!(sessions.retire_if_idle(old).unwrap());
    first_server.join();
    let deadline = std::time::Instant::now() + TIMEOUT;
    while sessions.diagnostics().retained != 0 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    let (session, second_server) = admitted_session(&service, 72_104);
    let current = sessions
        .register(
            thread,
            binding,
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        )
        .unwrap();
    assert!(!sessions.retire_if_idle(old).unwrap());
    assert_eq!(sessions.diagnostics().available, 1);
    drop(issued(&service, thread));
    assert!(sessions.retire_if_idle(current).unwrap());
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    second_server.join();
    assert!(!sessions.retire_if_idle(current).unwrap());
    drop(directory);
}
