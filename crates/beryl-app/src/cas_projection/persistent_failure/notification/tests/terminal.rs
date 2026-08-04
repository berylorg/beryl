use super::*;
#[test]
fn failed_completion_wakes_waiter_before_command_drain() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let gate = MasterCommandGate::new(service_generation, Some(fixture.notification.clone()));
    let permit = gate.authorizer().authorize().unwrap();
    let ticket = fixture
        .notification
        .verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        )
        .unwrap()
        .0;
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();
    let target = match fixture
        .notification
        .register_verification_join(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
            &ticket,
        )
        .unwrap()
    {
        VerificationJoinDisposition::Waiting(target) => target,
        disposition => panic!("expected failed-flight waiter, got {disposition:?}"),
    };
    recovery_receiver.recv().unwrap();
    let completion_probe = Arc::clone(&target);
    let notification = fixture.notification.clone();
    let waiter = std::thread::spawn(move || notification.wait_for_verification_completion(&target));

    fixture.faults.fail_next(FaultPoint::BeforeVerification);
    fixture.home.verify_health().unwrap_err();
    assert_eq!(fixture.home.health().state(), HomeHealthState::Failed);
    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Signaled
    );
    fixture.receiver.recv().unwrap();
    assert_eq!(
        completion_probe.outcome().unwrap(),
        Some(RecoverySupervisorFlightCompletion::FailedOrStale)
    );
    assert!(!permit.is_current());
    assert_eq!(
        waiter.join().unwrap().unwrap(),
        RecoverySupervisorFlightCompletion::FailedOrStale
    );
    drop(permit);
    gate.wait_until_drained().unwrap();
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn shutdown_completion_wakes_waiter_before_gate_close() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let gate = MasterCommandGate::new(service_generation, Some(fixture.notification.clone()));
    let ticket = fixture
        .notification
        .verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        )
        .unwrap()
        .0;
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();
    let target = match fixture
        .notification
        .register_verification_join(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
            &ticket,
        )
        .unwrap()
    {
        VerificationJoinDisposition::Waiting(target) => target,
        disposition => panic!("expected shutdown-flight waiter, got {disposition:?}"),
    };
    recovery_receiver.recv().unwrap();
    let notification = fixture.notification.clone();
    let waiter = std::thread::spawn(move || notification.wait_for_verification_completion(&target));

    fixture.notification.publish_shutdown_completion().unwrap();
    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::OrdinaryShutdown
    );
    assert_eq!(
        waiter.join().unwrap().unwrap(),
        RecoverySupervisorFlightCompletion::ShutdownOrUnavailable
    );
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn terminal_completion_settles_active_and_preissued_next_cells() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let (active, _) = fixture
        .notification
        .verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        )
        .unwrap();
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();
    recovery_receiver.recv().unwrap();
    fixture.home.verify_health().unwrap();
    let (next, _) = fixture
        .notification
        .verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        )
        .unwrap();
    assert!(!Arc::ptr_eq(&active, &next));

    fixture.notification.publish_shutdown_completion().unwrap();
    assert_eq!(
        fixture
            .notification
            .wait_for_verification_completion(&active)
            .unwrap(),
        RecoverySupervisorFlightCompletion::ShutdownOrUnavailable
    );
    assert_eq!(
        fixture
            .notification
            .wait_for_verification_completion(&next)
            .unwrap(),
        RecoverySupervisorFlightCompletion::ShutdownOrUnavailable
    );
    assert!(matches!(
        fixture.notification.verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        ),
        Err(LiveCommandAdmissionError::Unavailable)
    ));
    let (late_signal, _late_receiver) = mpsc::sync_channel(1);
    assert!(
        fixture
            .notification
            .attach_recovery_supervisor(late_signal)
            .is_err()
    );
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn completed_and_retired_ticket_remains_verified_current_during_registration() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let ticket = fixture
        .notification
        .verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        )
        .unwrap()
        .0;
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();
    recovery_receiver.recv().unwrap();
    let (observed_sender, observed_receiver) = mpsc::sync_channel(0);
    let (resume_sender, resume_receiver) = mpsc::sync_channel(0);
    fixture
        .notification
        .install_verification_join_observation_hook(&ticket, observed_sender, resume_receiver);
    let notification = fixture.notification.clone();
    let home = Arc::clone(&fixture.home);
    let join_ticket = Arc::clone(&ticket);
    let joiner = std::thread::spawn(move || {
        notification.register_verification_join(
            &home,
            home.home_id(),
            home_generation,
            service_generation,
            &join_ticket,
        )
    });

    observed_receiver.recv().unwrap();
    fixture.home.verify_health().unwrap();
    fixture
        .notification
        .publish_verified_current_completion()
        .unwrap()
        .unwrap();
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
    resume_sender.send(()).unwrap();

    let target = match joiner.join().unwrap().unwrap() {
        VerificationJoinDisposition::Waiting(target) => target,
        disposition => panic!("completed ticket lost its verified completion: {disposition:?}"),
    };
    assert!(Arc::ptr_eq(&target, &ticket));
    assert_eq!(
        fixture
            .notification
            .wait_for_verification_completion(&target)
            .unwrap(),
        RecoverySupervisorFlightCompletion::VerifiedCurrent
    );
}

#[test]
fn completed_and_retired_ticket_remains_shutdown_typed_during_registration() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let ticket = fixture
        .notification
        .verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        )
        .unwrap()
        .0;
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();
    recovery_receiver.recv().unwrap();
    let (observed_sender, observed_receiver) = mpsc::sync_channel(0);
    let (resume_sender, resume_receiver) = mpsc::sync_channel(0);
    fixture
        .notification
        .install_verification_join_observation_hook(&ticket, observed_sender, resume_receiver);
    let notification = fixture.notification.clone();
    let home = Arc::clone(&fixture.home);
    let join_ticket = Arc::clone(&ticket);
    let joiner = std::thread::spawn(move || {
        notification.register_verification_join(
            &home,
            home.home_id(),
            home_generation,
            service_generation,
            &join_ticket,
        )
    });

    observed_receiver.recv().unwrap();
    fixture.notification.publish_shutdown_completion().unwrap();
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
    resume_sender.send(()).unwrap();

    let target = match joiner.join().unwrap().unwrap() {
        VerificationJoinDisposition::Waiting(target) => target,
        disposition => panic!("completed ticket lost its shutdown completion: {disposition:?}"),
    };
    assert!(Arc::ptr_eq(&target, &ticket));
    assert_eq!(
        fixture
            .notification
            .wait_for_verification_completion(&target)
            .unwrap(),
        RecoverySupervisorFlightCompletion::ShutdownOrUnavailable
    );
}

#[test]
fn stale_election_completes_verifying_waiters_before_one_cut_wake() {
    const ELECTORS: usize = 8;

    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let ticket = fixture
        .notification
        .verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        )
        .unwrap()
        .0;
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();
    let target = match fixture
        .notification
        .register_verification_join(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
            &ticket,
        )
        .unwrap()
    {
        VerificationJoinDisposition::Waiting(target) => target,
        disposition => panic!("expected stale-flight waiter, got {disposition:?}"),
    };
    recovery_receiver.recv().unwrap();
    let barrier = Arc::new(Barrier::new(ELECTORS));
    let electors = (0..ELECTORS)
        .map(|_| {
            let notification = fixture.notification.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                notification.elect_and_publish_stale_completion()
            })
        })
        .collect::<Vec<_>>();

    for elector in electors {
        elector.join().unwrap().unwrap();
    }
    assert_eq!(
        target.outcome().unwrap(),
        Some(RecoverySupervisorFlightCompletion::FailedOrStale),
        "the exact stale election publishes terminal provider completion before waking the cut worker"
    );
    fixture.receiver.recv().unwrap();
    assert_eq!(fixture.receiver.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert_eq!(
        fixture
            .notification
            .wait_for_verification_completion(&target)
            .unwrap(),
        RecoverySupervisorFlightCompletion::FailedOrStale
    );
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn duplicate_and_post_cut_reentrant_signals_join_one_wake() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = FailedNotificationFixture::new(service_generation);

    assert_eq!(
        fixture.notification.service_generation(),
        service_generation
    );
    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Signaled
    );
    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Joined
    );
    fixture.notification.mark_cut_elected();
    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Joined
    );
    assert_eq!(fixture.receiver.try_iter().count(), 1);
}

#[test]
fn attachment_catches_up_failed_health_and_post_dequeue_duplicates_join() {
    let fixture = FailedNotificationFixture::new(ProjectionServiceGeneration::allocate().unwrap());
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();

    // The home was already failed before attachment, so attachment itself must publish the
    // one supervisor wake and elect the exact failure cut.
    recovery_receiver.recv().unwrap();
    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Joined
    );
    assert_eq!(
        recovery_receiver.try_recv(),
        Err(mpsc::TryRecvError::Disconnected),
        "terminal failed completion consumes the one-shot supervisor sender without queuing a duplicate wake"
    );
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn concurrent_exact_failed_signals_publish_one_wake() {
    const SIGNALERS: usize = 8;

    let fixture = FailedNotificationFixture::new(ProjectionServiceGeneration::allocate().unwrap());
    let barrier = Arc::new(Barrier::new(SIGNALERS));
    let signalers = (0..SIGNALERS)
        .map(|_| {
            let notification = fixture.notification.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                notification.notify()
            })
        })
        .collect::<Vec<_>>();
    let results = signalers
        .into_iter()
        .map(|signaler| signaler.join().expect("signaler remains available"))
        .collect::<Vec<_>>();

    assert_eq!(
        results
            .iter()
            .filter(|status| **status == PersistentFailureNotificationStatus::Signaled)
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|status| **status == PersistentFailureNotificationStatus::Joined)
            .count(),
        SIGNALERS - 1
    );
    assert_eq!(fixture.receiver.try_iter().count(), 1);
}

#[test]
fn typed_signal_rejects_foreign_home_identity_without_consuming_election() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = FailedNotificationFixture::new(service_generation);
    let mut foreign_home_id = *fixture.home.home_id().as_bytes();
    foreign_home_id[0] ^= u8::MAX;
    let (foreign, receiver) = persistent_failure_notification_channel(
        &fixture.home,
        BerylHomeId::from_bytes(foreign_home_id),
        fixture.home.health().generation().unwrap(),
        service_generation,
    );
    let gate = MasterCommandGate::new(service_generation, Some(foreign.clone()));

    assert_eq!(
        foreign.notify(),
        PersistentFailureNotificationStatus::NotFailed
    );
    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::OrdinaryShutdown
    );
    assert_eq!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty));
}
