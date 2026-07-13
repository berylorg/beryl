#![cfg(feature = "test-faults")]

mod support;

use std::{
    error::Error,
    io,
    panic::{AssertUnwindSafe, catch_unwind},
};

use beryl_home_store::{
    CommandError, HomeCommand, HomeHealthState, HomeOpenOptions, HomeRecoveryError,
    HomeSchemaVersion, HomeStore, PointReadLimit, ReadError,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use support::{AlphaDomain, BytesRecord, PutBytes};

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn put_command(
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

fn read_value(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<AlphaDomain>,
    key: u64,
) -> Option<Vec<u8>> {
    store
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &key,
            PointReadLimit::new(1_028).unwrap(),
        )
        .unwrap()
}

fn assert_io_kind(source: &(dyn Error + Send + Sync + 'static), expected: io::ErrorKind) {
    let source = source
        .downcast_ref::<io::Error>()
        .expect("deterministic fault source must remain an io::Error");
    assert_eq!(source.kind(), expected);
}

#[derive(Clone, Copy)]
enum ExpectedRecoveredState {
    Old,
    New,
    Either,
}

fn assert_recovered_state(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<AlphaDomain>,
    expected: ExpectedRecoveredState,
) {
    let home_revision = store.home_revision().unwrap().get();
    let domain_revision = store.domain_revision(domain).unwrap().get();
    let value = read_value(store, domain, 41);
    let old = home_revision == 1 && domain_revision == 1 && value.is_none();
    let new = home_revision == 2
        && domain_revision == 2
        && value.as_deref() == Some(b"panic boundary".as_slice());
    match expected {
        ExpectedRecoveredState::Old => assert!(old),
        ExpectedRecoveredState::New => assert!(new),
        ExpectedRecoveredState::Either => assert!(old || new),
    }
}

#[test]
fn controlled_commit_boundary_panics_fail_closed_and_recover_old_or_new() {
    for (point, expected) in [
        (FaultPoint::BeforeCommit, ExpectedRecoveredState::Old),
        (
            FaultPoint::AfterCommitBeforePersist,
            ExpectedRecoveredState::Either,
        ),
        (FaultPoint::AfterPersist, ExpectedRecoveredState::New),
    ] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let mut store = open(directory.path(), faults.clone());
        let alpha = store.register_domain::<AlphaDomain>().unwrap();
        let original_generation = store.health().generation().unwrap();

        faults.panic_next(point);
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let _ = store.execute(put_command(&store, alpha, 41, b"panic boundary"));
        }));
        assert!(panicked.is_err());
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        assert!(matches!(
            store.home_revision(),
            Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Failed
        ));

        let receipt = store.recover_same_home().unwrap();
        assert_eq!(receipt.generation().get(), original_generation.get() + 1);
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        let alpha = store.domain_handle::<AlphaDomain>().unwrap();
        assert_recovered_state(&store, alpha, expected);
    }
}

#[test]
fn exact_io_error_kinds_surface_at_the_commit_boundary() {
    for kind in [
        io::ErrorKind::StorageFull,
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::NotFound,
    ] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let mut store = open(directory.path(), faults.clone());
        let alpha = store.register_domain::<AlphaDomain>().unwrap();

        faults.fail_next_with_kind(FaultPoint::BeforeCommit, kind);
        let error = store
            .execute(put_command(&store, alpha, 9, b"must not commit"))
            .unwrap_err();
        match error {
            CommandError::Commit { source } => assert_io_kind(source.as_ref(), kind),
            other => panic!("unexpected command error: {other:?}"),
        }
        assert_eq!(store.health().state(), HomeHealthState::Verifying);

        store.verify_health().unwrap();
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        assert_eq!(store.home_revision().unwrap().get(), 1);
        assert_eq!(read_value(&store, alpha, 9), None);
    }
}

#[test]
fn surfaced_post_sync_all_failure_preserves_the_durable_new_state() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let generation = store.health().generation().unwrap();

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let error = store
        .execute(put_command(&store, alpha, 22, b"already durable"))
        .unwrap_err();
    match error {
        CommandError::Persistence { source } => {
            assert_io_kind(source.as_ref(), io::ErrorKind::StorageFull);
        }
        other => panic!("unexpected command error: {other:?}"),
    }
    assert_eq!(store.health().state(), HomeHealthState::Verifying);

    let health = store.verify_health().unwrap();
    assert_eq!(health.state(), HomeHealthState::Healthy);
    assert_eq!(health.generation(), Some(generation));
    assert_eq!(store.home_revision().unwrap().get(), 2);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 2);
    assert_eq!(
        read_value(&store, alpha, 22).as_deref(),
        Some(b"already durable".as_slice())
    );
}

#[test]
fn writer_panic_survives_persistent_recovery_faults_until_replacement_succeeds() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let original_generation = store.health().generation().unwrap();
    let mut poison_probe = Some(put_command(&store, alpha, 99, b"poison probe"));

    faults.panic_next(FaultPoint::BeforeCommit);
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        let _ = store.execute(put_command(&store, alpha, 1, b"panic"));
    }));
    assert!(panicked.is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    faults.fail_times_with_kind(FaultPoint::BeforeReopen, io::ErrorKind::PermissionDenied, 3);
    for _ in 0..3 {
        let error = store.recover_same_home().unwrap_err();
        match error {
            HomeRecoveryError::Layout { source } => {
                assert_io_kind(source.as_ref(), io::ErrorKind::PermissionDenied);
            }
            other => panic!("unexpected recovery error: {other:?}"),
        }
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        if let Some(probe) = poison_probe.take() {
            assert!(matches!(
                store.execute(probe),
                Err(CommandError::WriterPoisoned)
            ));
        }
    }

    let receipt = store.recover_same_home().unwrap();
    assert_eq!(receipt.generation().get(), original_generation.get() + 1);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);

    let alpha = store.domain_handle::<AlphaDomain>().unwrap();
    store
        .execute(put_command(&store, alpha, 2, b"writer usable"))
        .unwrap();
    assert_eq!(
        read_value(&store, alpha, 2).as_deref(),
        Some(b"writer usable".as_slice())
    );
}
