#![cfg(feature = "test-faults")]

mod support;

use std::{num::NonZeroU64, sync::Arc, thread, time::Duration};

use beryl_home_store::{
    CommandError, HealthVerificationError, HomeCommand, HomeHealthSnapshot, HomeHealthState,
    HomeOpenError, HomeOpenOptions, HomeRecoveryError, HomeSchemaVersion, HomeStore, ReadError,
    RecoveryRetrySchedule,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use support::{AlphaDomain, BytesRecord, PutBytes};

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn command(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<AlphaDomain>,
    key: u64,
    value: &[u8],
) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<AlphaDomain>::new(key, value.to_vec()),
        ))
        .unwrap();
    command
}

#[test]
fn opening_snapshot_has_no_generation() {
    let opening = HomeHealthSnapshot::opening();
    assert_eq!(opening.state(), HomeHealthState::Opening);
    assert_eq!(opening.generation(), None);
}

#[test]
fn surfaced_commit_failure_gates_reads_until_bounded_verification_succeeds() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let generation = store.health().generation().unwrap();

    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        store.execute(command(&store, alpha, 7, b"never committed")),
        Err(CommandError::Commit { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Verifying);
    assert!(matches!(
        store.home_revision(),
        Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Verifying
    ));

    let health = store.verify_health().unwrap();
    assert_eq!(health.state(), HomeHealthState::Healthy);
    assert_eq!(health.generation(), Some(generation));
    assert_eq!(store.home_revision().unwrap().get(), 1);
    assert_eq!(
        store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                alpha,
                &7,
                beryl_home_store::PointReadLimit::new(1_028).unwrap(),
            )
            .unwrap(),
        None
    );
}

#[test]
fn failed_verification_force_recovers_only_the_same_locked_home() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let home_id = store.home_id();
    let original_generation = store.health().generation().unwrap();
    let stale_command = command(&store, alpha, 91, b"stale");

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        store.execute(command(&store, alpha, 9, b"indeterminate")),
        Err(CommandError::Persistence { .. })
    ));
    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(matches!(
        store.verify_health(),
        Err(HealthVerificationError::Persistence { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(matches!(
        HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        )),
        Err(HomeOpenError::Busy { .. })
    ));

    let receipt = store.recover_same_home().unwrap();
    assert_eq!(receipt.generation().get(), original_generation.get() + 1);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(store.home_id(), home_id);
    assert!(matches!(
        store.execute(stale_command),
        Err(CommandError::ForeignDomain { .. })
    ));

    let alpha = store.domain_handle::<AlphaDomain>().unwrap();
    let revision = store.home_revision().unwrap().get();
    let value = store
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            alpha,
            &9,
            beryl_home_store::PointReadLimit::new(1_028).unwrap(),
        )
        .unwrap();
    assert!(
        (revision == 1 && value.is_none())
            || (revision == 2 && value.as_deref() == Some(b"indeterminate"))
    );
}

#[test]
fn recovery_is_single_flight_and_new_signals_join_the_active_attempt() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(store.execute(command(&store, alpha, 1, b"x")).is_err());
    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(store.verify_health().is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let block = faults.block_next(FaultPoint::BeforeReopen);
    let store = Arc::new(store);
    let recovering = Arc::clone(&store);
    let worker = thread::spawn(move || recovering.recover_same_home());
    assert!(block.wait_until_reached(Duration::from_secs(10)));
    assert_eq!(store.health().state(), HomeHealthState::Reopening);
    assert!(matches!(
        store.recover_same_home(),
        Err(HomeRecoveryError::InProgress {
            state: HomeHealthState::Reopening,
        })
    ));
    assert!(matches!(
        store.home_revision(),
        Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Reopening
    ));

    block.release();
    worker.join().unwrap().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn failed_recovery_can_retry_the_same_home_without_replacement_creation() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(store.execute(command(&store, alpha, 1, b"x")).is_err());
    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(store.verify_health().is_err());

    faults.fail_next(FaultPoint::BeforeReopen);
    assert!(matches!(
        store.recover_same_home(),
        Err(HomeRecoveryError::Layout { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    faults.fail_next(FaultPoint::AfterReopen);
    assert!(matches!(
        store.recover_same_home(),
        Err(HomeRecoveryError::Persistence { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    store.recover_same_home().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn recovery_rejects_a_removed_database_without_creating_a_replacement() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(store.execute(command(&store, alpha, 1, b"x")).is_err());
    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(store.verify_health().is_err());

    let database_path = store.database_path().to_path_buf();
    let moved_path = directory.path().join("moved-state");
    let block = faults.block_next(FaultPoint::BeforeReopen);
    let store = Arc::new(store);
    let recovering = Arc::clone(&store);
    let worker = thread::spawn(move || recovering.recover_same_home());
    assert!(block.wait_until_reached(Duration::from_secs(10)));
    std::fs::rename(&database_path, &moved_path).unwrap();
    block.release();

    assert!(matches!(
        worker.join().unwrap(),
        Err(HomeRecoveryError::Layout { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(!database_path.exists());
    assert!(moved_path.exists());
}

#[test]
fn retry_schedule_uses_the_accepted_bounded_delays() {
    let mut schedule = RecoveryRetrySchedule::default();
    let delays: Vec<_> = (0..7).map(|_| schedule.next_delay()).collect();
    assert_eq!(
        delays,
        [1_u64, 2, 5, 10, 30, 30, 30].map(Duration::from_secs)
    );
    schedule.reset();
    assert_eq!(schedule.next_delay(), Duration::from_secs(1));
}

#[test]
fn recovery_rejects_validator_disagreement_and_remains_failed() {
    use support::ValidatedDomain;

    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let domain = store.register_domain::<ValidatedDomain>().unwrap();
    store
        .execute(command_for_validated(&store, domain, b"reject"))
        .unwrap();

    faults.fail_next(FaultPoint::BeforeSidecarWrite);
    assert!(
        store
            .admit_sidecar(
                beryl_home_store::SidecarNamespace::new("fixture").unwrap(),
                b"sidecar",
                beryl_home_store::SidecarByteLimit::new(NonZeroU64::new(64).unwrap()),
            )
            .is_err()
    );
    assert!(matches!(
        store.verify_health(),
        Err(HealthVerificationError::Validation { .. })
    ));
    assert!(matches!(
        store.recover_same_home(),
        Err(HomeRecoveryError::Domain(_))
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

fn command_for_validated(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<support::ValidatedDomain>,
    value: &[u8],
) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<support::ValidatedDomain>::new(1, value.to_vec()),
        ))
        .unwrap();
    command
}

#[test]
fn sidecar_limit_type_remains_explicitly_nonzero() {
    let limit = beryl_home_store::SidecarByteLimit::new(NonZeroU64::new(1).unwrap());
    assert_eq!(limit.get(), 1);
}
