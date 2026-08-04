use std::{
    sync::{Arc, Barrier, atomic::AtomicBool, mpsc::sync_channel},
    time::Duration,
};

use super::*;

fn gate() -> MasterCommandGate {
    MasterCommandGate::new(ProjectionServiceGeneration::allocate().unwrap(), None)
}

fn close_for_failure(gate: &MasterCommandGate) {
    assert_eq!(
        gate.inner.observe_failure().unwrap(),
        FailureObservationElection::First
    );
    assert!(
        gate.close_for_persistent_failure(PersistentFailureGeneration::FIRST)
            .unwrap()
    );
}

#[test]
fn failure_close_invalidates_existing_permits_and_refuses_new_admission() {
    let gate = gate();
    let authorizer = gate.authorizer();
    let permit = authorizer.authorize().unwrap();

    close_for_failure(&gate);
    assert!(!permit.is_current());
    assert_eq!(
        authorizer.authorize().unwrap_err(),
        LiveCommandAdmissionError::Closed
    );
    drop(permit);
    gate.wait_until_drained().unwrap();
}

#[test]
fn close_waits_for_the_exact_pre_cut_permit_without_reopening() {
    let gate = gate();
    let permit = gate.authorizer().authorize().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let release = Arc::clone(&barrier);
    let waiter_gate = gate.clone();
    let waiter = std::thread::spawn(move || {
        release.wait();
        waiter_gate.wait_until_drained().unwrap();
    });

    close_for_failure(&gate);
    barrier.wait();
    std::thread::sleep(Duration::from_millis(10));
    assert!(!waiter.is_finished());
    drop(permit);
    waiter.join().unwrap();
    assert!(!gate.authorizer().is_open());
}

#[test]
fn failure_identity_and_stale_permit_are_exact_and_duplicate_close_joins() {
    let failure_gate = gate();
    let service = failure_gate.service_generation();
    let authorizer = failure_gate.authorizer();
    let stale_permit = authorizer.authorize().unwrap();
    let unaffected_gate = gate();
    let unaffected_permit = unaffected_gate.authorizer().authorize().unwrap();

    assert!(!failure_gate.matches_failure(service, PersistentFailureGeneration::FIRST));
    close_for_failure(&failure_gate);
    assert!(
        failure_gate
            .close_for_persistent_failure(PersistentFailureGeneration::FIRST)
            .unwrap()
    );
    assert!(failure_gate.matches_failure(service, PersistentFailureGeneration::FIRST));
    let foreign = ProjectionServiceGeneration::allocate().unwrap();
    assert!(!failure_gate.matches_failure(foreign, PersistentFailureGeneration::FIRST));
    assert!(!stale_permit.is_current());
    assert!(unaffected_permit.is_current());
    assert!(!unaffected_gate.matches_failure(service, PersistentFailureGeneration::FIRST));
}

#[test]
fn stale_permit_preserves_local_failure_after_later_persistent_election() {
    let gate = gate();
    let authorizer = gate.authorizer();
    let permit = authorizer.authorize().unwrap();

    gate.close_for_local_failure();
    assert_eq!(
        permit.status_exact().unwrap(),
        LiveCommandGateStatus::LocalFailure
    );
    assert_eq!(
        gate.inner.observe_failure().unwrap(),
        FailureObservationElection::First
    );
    assert!(
        gate.close_for_persistent_failure(PersistentFailureGeneration::FIRST)
            .unwrap()
    );
    assert_eq!(
        permit.status_exact().unwrap(),
        LiveCommandGateStatus::LocalFailure
    );
    assert_eq!(
        authorizer.status_exact().unwrap(),
        LiveCommandGateStatus::LocalFailure
    );
}

#[test]
fn local_failure_after_persistent_election_preserves_cut_but_dominates_status() {
    let gate = gate();
    let authorizer = gate.authorizer();
    let permit = authorizer.authorize().unwrap();

    close_for_failure(&gate);
    assert_eq!(
        permit.status_exact().unwrap(),
        LiveCommandGateStatus::PersistentFailure
    );
    gate.close_for_local_failure();
    assert_eq!(
        permit.status_exact().unwrap(),
        LiveCommandGateStatus::LocalFailure
    );
    assert!(gate.matches_failure(
        gate.service_generation(),
        PersistentFailureGeneration::FIRST
    ));
    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::PersistentFailure(PersistentFailureGeneration::FIRST)
    );
}

#[test]
fn authority_settlement_and_existing_permit_transfer_choose_one_exact_side_of_cut() {
    let gate = gate();
    let authorizer = gate.authorizer();
    let permit = authorizer.authorize().unwrap();

    assert_eq!(
        authorizer
            .settle_authority(|| "current", |_| "failure", || "closed")
            .unwrap(),
        "current"
    );
    close_for_failure(&gate);
    assert_eq!(
        permit
            .commit_or_transfer(|| "current", |_| "failure", || "closed")
            .unwrap(),
        "failure"
    );
    assert_eq!(
        authorizer
            .settle_authority(|| "current", |_| "failure", || "closed")
            .unwrap(),
        "failure"
    );
}

#[test]
fn pair_commit_handles_shared_and_distinct_gates_without_partial_commit() {
    let shared = gate();
    let first = shared.authorizer().authorize().unwrap();
    let second = shared.authorizer().authorize().unwrap();
    assert_eq!(first.commit_pair_if_current(&second, || 1).unwrap(), 1);

    let distinct = gate();
    let distinct_permit = distinct.authorizer().authorize().unwrap();
    close_for_failure(&distinct);
    assert_eq!(
        first
            .commit_pair_if_current(&distinct_permit, || 2)
            .unwrap_err(),
        LiveCommandAdmissionError::Closed
    );
}

#[test]
fn ordinary_shutdown_owner_is_sticky_against_a_late_failure_close() {
    let gate = gate();

    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::OrdinaryShutdown
    );
    assert!(
        !gate
            .close_for_persistent_failure(PersistentFailureGeneration::FIRST)
            .unwrap()
    );
    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::OrdinaryShutdown
    );
    assert!(!gate.authorizer().is_persistent_failure_cut());
}

#[test]
fn shutdown_joins_an_already_elected_failure_owner() {
    let gate = gate();
    close_for_failure(&gate);

    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::PersistentFailure(PersistentFailureGeneration::FIRST)
    );
    assert!(gate.authorizer().is_persistent_failure_cut());
}

#[cfg(feature = "test-faults")]
#[test]
fn ordinary_shutdown_winner_rejects_a_late_typed_failed_signal() {
    use super::super::notification::{
        PersistentFailureNotificationStatus, test_support::FailedNotificationFixture,
    };

    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = FailedNotificationFixture::new(service_generation);
    let gate = MasterCommandGate::new(service_generation, Some(fixture.notification.clone()));

    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::OrdinaryShutdown
    );
    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Unavailable
    );
    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::OrdinaryShutdown
    );
    assert!(!gate.authorizer().is_persistent_failure_cut());
    assert!(fixture.receiver.try_recv().is_err());
}

#[cfg(feature = "test-faults")]
#[test]
fn typed_failure_observed_first_makes_shutdown_join_the_failure_owner() {
    use super::super::notification::{
        PersistentFailureNotificationStatus, test_support::FailedNotificationFixture,
    };

    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = FailedNotificationFixture::new(service_generation);
    let gate = MasterCommandGate::new(service_generation, Some(fixture.notification.clone()));
    let stale_permit = gate.authorizer().authorize().unwrap();

    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Signaled
    );
    assert!(!stale_permit.is_current());
    assert_eq!(
        gate.close_for_shutdown(),
        MasterCommandGateCloseOwner::PersistentFailure(PersistentFailureGeneration::FIRST)
    );
    assert!(gate.matches_failure(service_generation, PersistentFailureGeneration::FIRST));
    assert!(gate.authorizer().is_persistent_failure_cut());
    assert_eq!(fixture.receiver.try_iter().count(), 1);
}

#[cfg(feature = "test-faults")]
#[test]
fn failure_observed_before_authority_commit_rejects_the_transition() {
    use super::super::notification::{
        PersistentFailureNotificationStatus, test_support::FailedNotificationFixture,
    };

    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = FailedNotificationFixture::new(service_generation);
    let gate = MasterCommandGate::new(service_generation, Some(fixture.notification.clone()));
    let permit = gate.authorizer().authorize().unwrap();
    let committed = AtomicBool::new(false);

    assert_eq!(
        fixture.notification.notify(),
        PersistentFailureNotificationStatus::Signaled
    );
    assert_eq!(
        permit.commit_if_current(|| committed.store(true, std::sync::atomic::Ordering::SeqCst)),
        Err(LiveCommandAdmissionError::Closed)
    );
    assert!(!committed.load(std::sync::atomic::Ordering::SeqCst));
}

#[cfg(feature = "test-faults")]
#[test]
fn authority_commit_observed_first_linearizes_before_failure_election() {
    use super::super::notification::{
        PersistentFailureNotificationStatus, test_support::FailedNotificationFixture,
    };

    let service_generation = ProjectionServiceGeneration::allocate().unwrap();
    let fixture = FailedNotificationFixture::new(service_generation);
    let gate = MasterCommandGate::new(service_generation, Some(fixture.notification.clone()));
    let permit = gate.authorizer().authorize().unwrap();
    let (commit_entered, commit_entered_rx) = sync_channel(0);
    let (release_commit, release_commit_rx) = sync_channel(0);
    let (commit_finished, commit_finished_rx) = sync_channel(0);
    let (release_permit, release_permit_rx) = sync_channel(0);
    let commit = std::thread::spawn(move || {
        let result = permit.commit_if_current(|| {
            commit_entered.send(()).unwrap();
            release_commit_rx.recv().unwrap();
            77
        });
        commit_finished.send(()).unwrap();
        release_permit_rx.recv().unwrap();
        result
    });
    commit_entered_rx.recv().unwrap();

    let notification = fixture.notification.clone();
    let (notification_started, notification_started_rx) = sync_channel(0);
    let notifier = std::thread::spawn(move || {
        notification_started.send(()).unwrap();
        notification.notify()
    });
    notification_started_rx.recv().unwrap();
    std::thread::sleep(Duration::from_millis(10));
    assert!(!notifier.is_finished());

    release_commit.send(()).unwrap();
    commit_finished_rx.recv().unwrap();
    assert_eq!(
        notifier.join().unwrap(),
        PersistentFailureNotificationStatus::Signaled
    );
    release_permit.send(()).unwrap();
    assert_eq!(commit.join().unwrap(), Ok(77));
    assert!(!gate.authorizer().is_open());
}
