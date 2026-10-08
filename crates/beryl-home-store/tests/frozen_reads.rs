#![cfg(feature = "test-faults")]

#[path = "frozen_reads/lifecycle.rs"]
mod lifecycle;
mod support;

use std::{path::Path, thread};

use beryl_home_store::{
    CommandCancellation, CursorDirection, CursorRange, CursorReadLimits, DomainHandle,
    FrozenHomeRead, FrozenReadAccessError, HomeCommand, HomeDomainRequirements, HomeHealthState,
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore, PointReadLimit, ReadError,
    set_frozen_read_limits_for_test,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::TempDir;

use support::{AlphaDomain, BytesRecord, PutBytes, committed};

struct Fixture {
    directory: TempDir,
    store: HomeStore,
    domain: DomainHandle<AlphaDomain>,
    faults: FaultController,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (store, domain) = open(directory.path(), faults.clone());
        Self {
            directory,
            store,
            domain,
            faults,
        }
    }

    fn capture(&self) -> FrozenHomeRead {
        self.store
            .capture_frozen_read(&CommandCancellation::new())
            .unwrap()
    }

    fn point(&self, read: &FrozenHomeRead, key: u64) -> Result<Option<Vec<u8>>, ReadError> {
        self.store
            .read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                read,
                &self.domain,
                &key,
                PointReadLimit::new(2_048).unwrap(),
            )
    }
}

fn open(path: &Path, faults: FaultController) -> (HomeStore, DomainHandle<AlphaDomain>) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let domain = candidate.register_domain::<AlphaDomain>().unwrap();
    let store = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    (store, domain)
}

fn put(store: &HomeStore, domain: &DomainHandle<AlphaDomain>, key: u64, value: &[u8]) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<AlphaDomain>::new(key, value.to_vec()),
        ))
        .unwrap();
    committed(store.execute(command));
}

#[test]
fn fixed_points_pages_and_revisions_survive_real_concurrent_commits() {
    let fixture = Fixture::new();
    for key in 1..=3 {
        put(&fixture.store, &fixture.domain, key, &[key as u8]);
    }
    let read = fixture.capture();
    let home_revision = read.home_revision();
    let domain_revision = fixture
        .store
        .frozen_domain_revision(&read, &fixture.domain)
        .unwrap();
    let first = fixture
        .store
        .read_frozen_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &read,
            &fixture.domain,
            &CursorRange::closed(1, 4),
            CursorDirection::Forward,
            CursorReadLimits::new(2, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(first.records().len(), 2);
    assert!(first.has_more());
    let writing = fixture.store.service_reference();
    let writing_domain = fixture.domain.clone();
    thread::spawn(move || {
        put(&writing, &writing_domain, 1, b"changed");
        put(&writing, &writing_domain, 4, b"new");
    })
    .join()
    .unwrap();
    assert!(fixture.store.home_revision().unwrap() > home_revision);
    assert_eq!(fixture.point(&read, 1).unwrap(), Some(vec![1]));
    assert_eq!(fixture.point(&read, 4).unwrap(), None);
    assert_eq!(
        fixture
            .store
            .frozen_domain_revision(&read, &fixture.domain)
            .unwrap(),
        domain_revision
    );
    let second = fixture
        .store
        .read_frozen_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &read,
            &fixture.domain,
            &CursorRange::closed(3, 4),
            CursorDirection::Forward,
            CursorReadLimits::new(2, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(second.records().len(), 1);
    assert_eq!(*second.records()[0].key(), 3);
    assert!(!second.has_more());
    fixture.store.release_frozen_read(&read).unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn independent_retention_keeps_exact_snapshot_after_publisher_release() {
    let fixture = Fixture::new();
    put(&fixture.store, &fixture.domain, 1, b"original");
    let published = fixture.capture();
    let cloned = published.clone();
    let retained = fixture
        .store
        .retain_frozen_read(&published, &CommandCancellation::new())
        .unwrap();
    assert_ne!(published, retained);
    assert_eq!(retained.home_revision(), published.home_revision());
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 2);
    put(&fixture.store, &fixture.domain, 1, b"later");
    fixture.store.release_frozen_read(&published).unwrap();
    assert!(matches!(
        fixture.point(&cloned, 1),
        Err(ReadError::FrozenRead(FrozenReadAccessError::Released))
    ));
    assert_eq!(
        fixture.point(&retained, 1).unwrap(),
        Some(b"original".to_vec())
    );
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 1);
    fixture.store.release_frozen_read(&retained).unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn finite_slots_cancellation_and_identity_exhaustion_do_not_fail_home() {
    let fixture = Fixture::new();
    set_frozen_read_limits_for_test(&fixture.store, 2, 1).unwrap();
    let read = fixture.capture();
    let retained = fixture
        .store
        .retain_frozen_read(&read, &CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        fixture
            .store
            .capture_frozen_read(&CommandCancellation::new()),
        Err(ReadError::FrozenRead(FrozenReadAccessError::Saturated {
            maximum: 2
        }))
    ));
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        fixture.store.retain_frozen_read(&read, &cancelled),
        Err(ReadError::FrozenRead(FrozenReadAccessError::Cancelled))
    ));
    fixture.store.release_frozen_read(&retained).unwrap();
    fixture.store.release_frozen_read(&read).unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    set_frozen_read_limits_for_test(&fixture.store, 2, u64::MAX).unwrap();
    assert!(matches!(
        fixture
            .store
            .capture_frozen_read(&CommandCancellation::new()),
        Err(ReadError::FrozenRead(
            FrozenReadAccessError::IdentityExhausted
        ))
    ));
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(fixture.store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn foreign_identity_domain_and_caller_limits_reject_without_health_damage() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    put(&fixture.store, &fixture.domain, 1, b"bounded");
    let read = fixture.capture();
    assert!(matches!(
        foreign.point(&read, 1),
        Err(ReadError::FrozenRead(FrozenReadAccessError::Foreign))
    ));
    assert!(matches!(
        fixture
            .store
            .read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &read,
                &foreign.domain,
                &1,
                PointReadLimit::new(128).unwrap(),
            ),
        Err(ReadError::ForeignDomain { .. })
    ));
    assert!(matches!(
        fixture
            .store
            .read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &read,
                &fixture.domain,
                &1,
                PointReadLimit::new(1).unwrap(),
            ),
        Err(ReadError::BoundExceeded { .. })
    ));
    assert_eq!(fixture.store.health().state(), HomeHealthState::Healthy);
    assert_eq!(foreign.store.health().state(), HomeHealthState::Healthy);
    fixture.store.release_frozen_read(&read).unwrap();
}

#[test]
fn failed_capture_and_retention_confirmation_release_reserved_slots() {
    let fixture = Fixture::new();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        fixture
            .store
            .capture_frozen_read(&CommandCancellation::new())
            .is_err()
    );
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    let fixture = Fixture::new();
    let read = fixture.capture();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        fixture
            .store
            .retain_frozen_read(&read, &CommandCancellation::new())
            .is_err()
    );
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 1);
    fixture.store.release_frozen_read(&read).unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn stored_envelope_failure_preserves_cleanup_access() {
    let fixture = Fixture::new();
    let mut malformed = 1_u32.to_be_bytes().to_vec();
    malformed.extend_from_slice(&vec![7; 1_025]);
    fixture
        .store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &fixture.domain,
            &1_u64.to_be_bytes(),
            &malformed,
        )
        .unwrap();
    let read = fixture.capture();
    assert!(matches!(
        fixture.point(&read, 1),
        Err(ReadError::InvalidStoredValueSize { .. })
    ));
    fixture.store.release_frozen_read(&read).unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}
