use std::sync::{Arc, Barrier, mpsc};

use beryl_home_store::{HomeHealthState, test_faults::FaultPoint};
use beryl_model::BerylHomeId;

use super::{
    PersistentFailureNotificationStatus, ProjectionServiceGeneration,
    RecoverySupervisorFlightCompletion, VerificationJoinDisposition,
    persistent_failure_notification_channel,
    test_support::{FailedNotificationFixture, VerifyingNotificationFixture},
};
use crate::cas_projection::persistent_failure::{
    LiveCommandAdmissionError, MasterCommandGate, MasterCommandGateCloseOwner,
};

mod terminal;

#[test]
fn verification_registration_signals_and_receives_exact_completion() {
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
        disposition => panic!("expected a waiting registration, got {disposition:?}"),
    };
    recovery_receiver.recv().unwrap();
    fixture.home.verify_health().unwrap();
    assert!(
        fixture
            .notification
            .publish_verified_current_completion()
            .unwrap()
            .is_some()
    );
    assert_eq!(
        fixture
            .notification
            .wait_for_verification_completion(&target)
            .unwrap(),
        RecoverySupervisorFlightCompletion::VerifiedCurrent
    );
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn verification_completion_wakes_every_registered_joiner() {
    const JOINERS: usize = 6;

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
    let targets = (0..JOINERS)
        .map(|_| {
            match fixture
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
                disposition => panic!("expected waiting joiner, got {disposition:?}"),
            }
        })
        .collect::<Vec<_>>();
    assert!(
        targets
            .iter()
            .all(|target| Arc::ptr_eq(target, &targets[0]))
    );
    let barrier = Arc::new(Barrier::new(JOINERS + 1));
    let joiners = targets
        .into_iter()
        .map(|target| {
            let notification = fixture.notification.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                notification.wait_for_verification_completion(&target)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    fixture.home.verify_health().unwrap();
    fixture
        .notification
        .publish_verified_current_completion()
        .unwrap();
    for joiner in joiners {
        assert_eq!(
            joiner.join().unwrap().unwrap(),
            RecoverySupervisorFlightCompletion::VerifiedCurrent
        );
    }
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn adjacent_flight_cells_keep_their_immutable_exact_outcomes() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let (first_ticket, _) = fixture
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
    let first = match fixture
        .notification
        .register_verification_join(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
            &first_ticket,
        )
        .unwrap()
    {
        VerificationJoinDisposition::Waiting(target) => target,
        disposition => panic!("expected first flight cell, got {disposition:?}"),
    };
    fixture.home.verify_health().unwrap();
    fixture
        .notification
        .publish_verified_current_completion()
        .unwrap();

    fixture.enter_next_verifying();
    let (second_ticket, _) = fixture
        .notification
        .verification_completion_ticket(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
        )
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &second_ticket));
    let second = match fixture
        .notification
        .register_verification_join(
            &fixture.home,
            fixture.home.home_id(),
            home_generation,
            service_generation,
            &second_ticket,
        )
        .unwrap()
    {
        VerificationJoinDisposition::Waiting(target) => target,
        disposition => panic!("expected second flight cell, got {disposition:?}"),
    };
    fixture.notification.finish_recovery_supervisor_flight(true);
    recovery_receiver.recv().unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeVerification);
    fixture.home.verify_health().unwrap_err();
    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Signaled
    );

    assert_eq!(
        fixture
            .notification
            .wait_for_verification_completion(&first)
            .unwrap(),
        RecoverySupervisorFlightCompletion::VerifiedCurrent
    );
    assert_eq!(
        fixture
            .notification
            .wait_for_verification_completion(&second)
            .unwrap(),
        RecoverySupervisorFlightCompletion::FailedOrStale
    );
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn completion_before_wait_is_not_missed_by_the_pre_command_witness() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let gate = MasterCommandGate::new(service_generation, Some(fixture.notification.clone()));
    let permit = gate.authorizer().authorize().unwrap();
    let witness = permit
        .verification_join(&fixture.home, fixture.home.home_id(), home_generation)
        .unwrap();
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();
    recovery_receiver.recv().unwrap();
    fixture.home.verify_health().unwrap();
    fixture
        .notification
        .publish_verified_current_completion()
        .unwrap();

    assert_eq!(witness.wait_after_ambiguous(), Ok(true));
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}

#[test]
fn exact_pre_operation_verifying_joins_then_requires_a_fresh_command_witness() {
    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = VerifyingNotificationFixture::new(service_generation);
    let home_generation = fixture.home.health().generation().unwrap();
    let gate = MasterCommandGate::new(service_generation, Some(fixture.notification.clone()));
    let permit = gate.authorizer().authorize().unwrap();
    let (recovery_signal, recovery_receiver) = mpsc::sync_channel(1);
    fixture
        .notification
        .attach_recovery_supervisor(recovery_signal)
        .unwrap();
    let home = Arc::clone(&fixture.home);
    let notification = fixture.notification.clone();
    let supervisor = std::thread::spawn(move || {
        recovery_receiver.recv().unwrap();
        home.verify_health().unwrap();
        notification.publish_verified_current_completion().unwrap();
    });

    permit
        .await_current_or_verification(&fixture.home, fixture.home.home_id(), home_generation)
        .unwrap();
    supervisor.join().unwrap();
    let fresh = permit
        .verification_join(&fixture.home, fixture.home.home_id(), home_generation)
        .unwrap();
    assert_eq!(fresh.wait_after_ambiguous(), Ok(false));
    fixture
        .notification
        .finish_recovery_supervisor_flight(false);
}
