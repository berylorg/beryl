#![cfg(feature = "test-faults")]

mod support;

use std::{sync::Arc, thread, time::Duration};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackSource, DomainRegistrationError,
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore, PointReadLimit,
    ReadError,
    test_faults::{FaultController, FaultPoint, PersistedCorruptionError},
};
use tempfile::tempdir;

use support::{AlphaDomain, BytesRecord, PutBytes};

const MAX_STORED_VALUE_BYTES: usize = 1_028;

fn encoded_value(payload: &[u8]) -> Vec<u8> {
    let mut encoded = 1_u32.to_be_bytes().to_vec();
    encoded.extend_from_slice(payload);
    encoded
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn put(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<AlphaDomain>,
    key: u64,
    value: Vec<u8>,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<AlphaDomain>::new(key, value),
        ))
        .unwrap();
    store.execute(command).unwrap();
}

fn assert_failed_gate(store: &HomeStore) {
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(matches!(
        store.home_revision(),
        Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Failed
    ));
}

#[test]
fn point_read_fails_closed_on_persisted_oversized_value_before_caller_budget() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let domain = store.register_domain::<AlphaDomain>().unwrap();
    let oversized = encoded_value(&vec![7; 1_025]);
    assert_eq!(oversized.len(), MAX_STORED_VALUE_BYTES + 1);

    store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &1_u64.to_be_bytes(),
            &oversized,
        )
        .unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);

    assert!(matches!(
        store.read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &1,
            PointReadLimit::new(1).unwrap(),
        ),
        Err(ReadError::InvalidStoredValueSize {
            maximum: MAX_STORED_VALUE_BYTES,
            actual,
            ..
        }) if actual == MAX_STORED_VALUE_BYTES + 1
    ));
    assert_failed_gate(&store);
}

#[test]
fn cursor_read_fails_closed_on_persisted_oversized_key() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let domain = store.register_domain::<AlphaDomain>().unwrap();
    let oversized_key = [0_u8; 9];

    store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &oversized_key,
            &encoded_value(b"valid"),
        )
        .unwrap();

    assert!(matches!(
        store.read_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &CursorRange::closed(0, u64::MAX),
            CursorDirection::Forward,
            CursorReadLimits::new(4, 2_048).unwrap(),
        ),
        Err(ReadError::InvalidStoredKeySize {
            maximum: 8,
            actual: 9,
            ..
        })
    ));
    assert_failed_gate(&store);
}

#[test]
fn cursor_read_fails_closed_on_persisted_oversized_value() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let domain = store.register_domain::<AlphaDomain>().unwrap();
    let oversized = encoded_value(&vec![9; 1_025]);

    store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &3_u64.to_be_bytes(),
            &oversized,
        )
        .unwrap();

    assert!(matches!(
        store.read_cursor::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &CursorRange::closed(3, 3),
            CursorDirection::Forward,
            CursorReadLimits::new(1, 1).unwrap(),
        ),
        Err(ReadError::InvalidStoredValueSize {
            maximum: MAX_STORED_VALUE_BYTES,
            actual,
            ..
        }) if actual == MAX_STORED_VALUE_BYTES + 1
    ));
    assert_failed_gate(&store);
}

#[test]
fn persisted_corruption_seam_rejects_valid_or_empty_records() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let domain = store.register_domain::<AlphaDomain>().unwrap();

    assert!(matches!(
        store.inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &1_u64.to_be_bytes(),
            &encoded_value(b"valid"),
        ),
        Err(PersistedCorruptionError::CodecAcceptedEnvelope { .. })
    ));
    assert!(matches!(
        store.inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &[],
            &vec![0; MAX_STORED_VALUE_BYTES + 1],
        ),
        Err(PersistedCorruptionError::EmptyKey)
    ));
    assert!(matches!(
        store.inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &vec![0; usize::from(u16::MAX) + 1],
            &[],
        ),
        Err(PersistedCorruptionError::FixtureKeyBoundExceeded { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn persisted_corruption_seam_completes_a_durable_record_barrier() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let domain = store.register_domain::<AlphaDomain>().unwrap();
    store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &[0_u8; 9],
            &encoded_value(b"durable"),
        )
        .unwrap();
    store.close().unwrap();

    let mut reopened = support::open_home(directory.path());
    assert!(matches!(
        reopened.register_domain::<AlphaDomain>(),
        Err(DomainRegistrationError::ValidationAccess {
            source: DomainCallbackSource::Read(ReadError::InvalidStoredKeySize {
                maximum: 8,
                actual: 9,
                ..
            }),
            ..
        })
    ));
    assert_eq!(reopened.health().state(), HomeHealthState::Failed);
}

#[test]
fn admitted_success_rejects_after_concurrent_structural_read_failure() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let domain = store.register_domain::<AlphaDomain>().unwrap();
    put(&store, domain, 1, b"coherent".to_vec());
    store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &2_u64.to_be_bytes(),
            &encoded_value(&vec![5; 1_025]),
        )
        .unwrap();

    let blocked = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let store = Arc::new(store);
    let reading = Arc::clone(&store);
    let worker = thread::spawn(move || {
        reading.read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &1,
            PointReadLimit::new(2_048).unwrap(),
        )
    });
    assert!(blocked.wait_until_reached(Duration::from_secs(10)));

    assert!(matches!(
        store.read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &2,
            PointReadLimit::new(2_048).unwrap(),
        ),
        Err(ReadError::InvalidStoredValueSize { .. })
    ));
    blocked.release();

    assert!(matches!(
        worker.join().unwrap(),
        Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Failed
    ));
    assert_failed_gate(&store);
}
