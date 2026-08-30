use std::{
    convert::Infallible,
    error::Error,
    fmt,
    marker::PhantomData,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use beryl_home_store::{
    DomainAttachmentAccessError, DomainHandleError, DomainMutation, DomainReader,
    DomainRegistrationError, DomainRegistrationReader, DomainRuntimeAttachment,
    DomainSchemaVersion, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, KeyspaceSchemaVersion, MutationBuilder, PointReadLimit, ReadError,
    ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
    test_faults::{FaultController, FaultPoint, capability_with_test_attachment_type},
};
use tempfile::tempdir;

mod support;
use support::{FixtureMutationError, committed};

static NEXT_IDENTITY: AtomicUsize = AtomicUsize::new(1);
static DOMAIN_A_STATS: AttachmentStats = AttachmentStats::new();
static DOMAIN_B_STATS: AttachmentStats = AttachmentStats::new();
static IMPOSTOR_STATS: AttachmentStats = AttachmentStats::new();
static FAIL_DOMAIN_B: AtomicBool = AtomicBool::new(false);
static ATTACHMENT_READ_LIMIT: AtomicUsize = AtomicUsize::new(16);

struct AttachmentStats {
    constructed: AtomicUsize,
    retired: AtomicUsize,
    dropped: AtomicUsize,
}

impl AttachmentStats {
    const fn new() -> Self {
        Self {
            constructed: AtomicUsize::new(0),
            retired: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
        }
    }

    fn reset(&self) {
        self.constructed.store(0, Ordering::SeqCst);
        self.retired.store(0, Ordering::SeqCst);
        self.dropped.store(0, Ordering::SeqCst);
    }

    fn counts(&self) -> (usize, usize, usize) {
        (
            self.constructed.load(Ordering::SeqCst),
            self.retired.load(Ordering::SeqCst),
            self.dropped.load(Ordering::SeqCst),
        )
    }
}

struct TestAttachment {
    identity: usize,
    persisted_value: Option<u64>,
    stats: &'static AttachmentStats,
}

impl TestAttachment {
    fn new<D: StorageDomain>(
        stats: &'static AttachmentStats,
        reader: &DomainRegistrationReader<'_, D>,
    ) -> Result<Self, FactoryError>
    where
        TestCodec<D>: RecordCodec<D, Key = u64, Value = u64>,
    {
        let persisted_value = reader
            .point::<TestCodec<D>>(
                &7,
                PointReadLimit::new(ATTACHMENT_READ_LIMIT.load(Ordering::SeqCst)).unwrap(),
            )
            .map_err(FactoryError::Read)?;
        stats.constructed.fetch_add(1, Ordering::SeqCst);
        Ok(Self {
            identity: NEXT_IDENTITY.fetch_add(1, Ordering::SeqCst),
            persisted_value,
            stats,
        })
    }
}

impl DomainRuntimeAttachment for TestAttachment {
    fn retire(&mut self) {
        self.stats.retired.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for TestAttachment {
    fn drop(&mut self) {
        self.stats.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Debug)]
enum FactoryError {
    Configured,
    Read(ReadError),
}

impl fmt::Display for FactoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configured => formatter.write_str("configured attachment factory failure"),
            Self::Read(source) => {
                write!(formatter, "attachment registration read failed: {source}")
            }
        }
    }
}

impl Error for FactoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Configured => None,
            Self::Read(source) => Some(source),
        }
    }
}

struct DomainA;
struct DomainB;
struct ImpostorDomain;
struct TestCodec<D>(PhantomData<fn(D) -> D>);

struct PutValue<D> {
    key: u64,
    value: u64,
    _typed: PhantomData<fn(D) -> D>,
}

impl<D> PutValue<D> {
    fn new(key: u64, value: u64) -> Self {
        Self {
            key,
            value,
            _typed: PhantomData,
        }
    }
}

impl<D: StorageDomain> DomainMutation<D> for PutValue<D>
where
    TestCodec<D>: RecordCodec<D, Key = u64, Value = u64>,
{
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(self, _reader: &DomainReader<'_, D>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, D>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<TestCodec<D>>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error> {
        mutations.put::<TestCodec<D>>(&prepared.key, &prepared.value)?;
        Ok(())
    }
}

impl<D: StorageDomain> RecordCodec<D> for TestCodec<D> {
    type Key = u64;
    type Value = u64;
    type Error = Infallible;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 8;
    const MAX_VALUE_BYTES: usize = 8;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.to_be_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(encoded);
        Ok(u64::from_be_bytes(bytes))
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(value.to_be_bytes().to_vec())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(encoded);
        Ok(u64::from_be_bytes(bytes))
    }
}

macro_rules! attachment_domain {
    ($domain:ident, $name:literal, $stats:ident, $error:ty, $factory:expr) => {
        impl StorageDomain for $domain {
            const NAME: &'static str = $name;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] =
                &[RecordFamily::new::<TestCodec<Self>>(
                    KeyspaceSchemaVersion::new(1),
                )];
            type ValidationError = Infallible;
            type RuntimeAttachment = TestAttachment;
            type RuntimeAttachmentError = $error;

            fn create_runtime_attachment(
                reader: &DomainRegistrationReader<'_, Self>,
            ) -> Result<Self::RuntimeAttachment, Self::RuntimeAttachmentError> {
                ($factory)(reader)
            }

            fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                Ok(())
            }
        }
    };
}

attachment_domain!(
    DomainA,
    "runtime-a",
    DOMAIN_A_STATS,
    FactoryError,
    |reader| TestAttachment::new(&DOMAIN_A_STATS, reader)
);
attachment_domain!(
    DomainB,
    "runtime-b",
    DOMAIN_B_STATS,
    FactoryError,
    |reader| {
        if FAIL_DOMAIN_B.load(Ordering::SeqCst) {
            Err(FactoryError::Configured)
        } else {
            TestAttachment::new(&DOMAIN_B_STATS, reader)
        }
    }
);
attachment_domain!(
    ImpostorDomain,
    "runtime-a",
    IMPOSTOR_STATS,
    FactoryError,
    |reader| TestAttachment::new(&IMPOSTOR_STATS, reader)
);

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn fail_store(store: HomeStore, faults: &FaultController) -> HomeStore {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        store.home_revision(),
        Err(ReadError::Storage { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    store
}

fn reset() {
    DOMAIN_A_STATS.reset();
    DOMAIN_B_STATS.reset();
    IMPOSTOR_STATS.reset();
    FAIL_DOMAIN_B.store(false, Ordering::SeqCst);
    ATTACHMENT_READ_LIMIT.store(16, Ordering::SeqCst);
}

fn put<D: StorageDomain>(store: &HomeStore, domain: &beryl_home_store::DomainHandle<D>, value: u64)
where
    TestCodec<D>: RecordCodec<D, Key = u64, Value = u64>,
{
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutValue::<D>::new(7, value),
        ))
        .unwrap();
    committed(store.execute(command));
}

#[test]
fn registration_attachments_read_only_their_persisted_domain_on_reopen() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let domain_a = store.register_domain::<DomainA>().unwrap();
    let domain_b = store.register_domain::<DomainB>().unwrap();
    put(&store, &domain_a, 41);
    put(&store, &domain_b, 99);
    store.close().unwrap();

    let mut store = open(directory.path(), faults);
    let domain_a = store.register_domain::<DomainA>().unwrap();
    let domain_b = store.register_domain::<DomainB>().unwrap();
    let value_a = store
        .with_domain_attachment(&domain_a.attachment_capability(), |attachment| {
            attachment.persisted_value
        })
        .unwrap();
    let value_b = store
        .with_domain_attachment(&domain_b.attachment_capability(), |attachment| {
            attachment.persisted_value
        })
        .unwrap();
    assert_eq!(value_a, Some(41));
    assert_eq!(value_b, Some(99));
    store.close().unwrap();
}

#[test]
fn failed_initial_attachment_construction_publishes_no_slot_and_allows_retry() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);

    FAIL_DOMAIN_B.store(true, Ordering::SeqCst);
    assert!(matches!(
        store.register_domain::<DomainB>(),
        Err(DomainRegistrationError::AttachmentConstruction {
            domain: "runtime-b",
            ..
        })
    ));
    assert!(matches!(
        store.domain_handle::<DomainB>(),
        Err(DomainHandleError::NotRegistered {
            domain: "runtime-b"
        })
    ));
    assert_eq!(DOMAIN_B_STATS.counts(), (0, 0, 0));

    FAIL_DOMAIN_B.store(false, Ordering::SeqCst);
    let handle = store.register_domain::<DomainB>().unwrap();
    assert_eq!(DOMAIN_B_STATS.counts(), (1, 0, 0));
    drop(handle);
    store.close().unwrap();
    assert_eq!(DOMAIN_B_STATS.counts(), (1, 1, 1));
}

#[test]
fn structural_registration_read_failure_fails_health_before_publication() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let domain = store.register_domain::<DomainA>().unwrap();
    put(&store, &domain, 41);
    store
        .inject_persisted_corrupt_record::<DomainA, TestCodec<DomainA>>(
            &domain,
            &7_u64.to_be_bytes(),
            &2_u32.to_be_bytes(),
        )
        .unwrap();
    store.close().unwrap();

    let mut store = open(directory.path(), faults);
    assert!(matches!(
        store.register_domain::<DomainA>(),
        Err(DomainRegistrationError::AttachmentConstruction {
            domain: "runtime-a",
            ..
        })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(matches!(
        store.domain_handle::<DomainA>(),
        Err(DomainHandleError::HealthGate(_))
    ));
    assert_eq!(DOMAIN_A_STATS.counts(), (1, 1, 1));
    store.close().unwrap();
}

#[test]
fn bounded_registration_read_failure_keeps_health_and_allows_retry() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let domain = store.register_domain::<DomainA>().unwrap();
    put(&store, &domain, 41);
    store.close().unwrap();

    ATTACHMENT_READ_LIMIT.store(1, Ordering::SeqCst);
    let mut store = open(directory.path(), faults);
    assert!(matches!(
        store.register_domain::<DomainA>(),
        Err(DomainRegistrationError::AttachmentConstruction {
            domain: "runtime-a",
            ..
        })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert!(matches!(
        store.domain_handle::<DomainA>(),
        Err(DomainHandleError::NotRegistered {
            domain: "runtime-a"
        })
    ));

    ATTACHMENT_READ_LIMIT.store(16, Ordering::SeqCst);
    let domain = store.register_domain::<DomainA>().unwrap();
    let persisted = store
        .with_domain_attachment(&domain.attachment_capability(), |attachment| {
            attachment.persisted_value
        })
        .unwrap();
    assert_eq!(persisted, Some(41));
    store.close().unwrap();
}

#[test]
fn one_slot_owns_one_attachment_across_clone_and_reacquisition() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);
    let handle = store.register_domain::<DomainA>().unwrap();
    let cloned = handle.clone();
    let reacquired = store.domain_handle::<DomainA>().unwrap();
    let capability = handle.attachment_capability();
    let cloned_capability = cloned.attachment_capability();
    let reacquired_capability = reacquired.attachment_capability();

    let first = store
        .with_domain_attachment(&capability, |attachment| attachment.identity)
        .unwrap();
    let second = store
        .with_domain_attachment(&cloned_capability, |attachment| attachment.identity)
        .unwrap();
    let third = store
        .with_domain_attachment(&reacquired_capability, |attachment| attachment.identity)
        .unwrap();
    assert_eq!((first, first), (second, third));
    assert_eq!(DOMAIN_A_STATS.counts(), (1, 0, 0));

    drop(handle);
    drop(cloned);
    drop(reacquired);
    drop(capability);
    drop(cloned_capability);
    drop(reacquired_capability);
    assert_eq!(DOMAIN_A_STATS.counts(), (1, 0, 0));
    store.close().unwrap();
    assert_eq!(DOMAIN_A_STATS.counts(), (1, 1, 1));
}

#[test]
fn recovery_retires_old_attachment_and_rejects_stale_views() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let stale_handle = store.register_domain::<DomainA>().unwrap();
    let stale_capability = stale_handle.attachment_capability();
    let old_identity = store
        .with_domain_attachment(&stale_capability, |attachment| attachment.identity)
        .unwrap();

    let failed = fail_store(store, &faults);
    let candidate = failed.recover_same_home().unwrap();
    assert_eq!(DOMAIN_A_STATS.counts(), (2, 1, 1));
    let current_handle = candidate.domain_handle::<DomainA>().unwrap();
    let current_capability = current_handle.attachment_capability();
    assert!(matches!(
        candidate.with_domain_attachment(&stale_capability, |_| ()),
        Err(DomainAttachmentAccessError::StaleOrForeign)
    ));
    let fresh_identity = candidate
        .with_domain_attachment(&current_capability, |attachment| attachment.identity)
        .unwrap();
    assert_ne!(old_identity, fresh_identity);

    let recovered = candidate.publish();
    assert!(matches!(
        recovered.with_domain_attachment(&stale_capability, |_| ()),
        Err(DomainAttachmentAccessError::StaleOrForeign)
    ));
    assert!(recovered.domain_revision(&stale_handle).is_err());
    assert_eq!(recovered.domain_revision(&current_handle).unwrap().get(), 1);
    recovered.close().unwrap();
    assert_eq!(DOMAIN_A_STATS.counts(), (2, 2, 2));
}

#[test]
fn candidate_abort_retires_attachment_before_a_fresh_retry() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    store.register_domain::<DomainA>().unwrap();
    let failed = fail_store(store, &faults);
    let candidate = failed.recover_same_home().unwrap();
    let candidate_handle = candidate.domain_handle::<DomainA>().unwrap();
    let candidate_capability = candidate_handle.attachment_capability();
    let candidate_identity = candidate
        .with_domain_attachment(&candidate_capability, |attachment| attachment.identity)
        .unwrap();
    let failed = candidate.abort();
    assert_eq!(DOMAIN_A_STATS.counts(), (2, 2, 2));

    let candidate = failed.recover_same_home().unwrap();
    let retry_capability = candidate
        .domain_handle::<DomainA>()
        .unwrap()
        .attachment_capability();
    let retry_identity = candidate
        .with_domain_attachment(&retry_capability, |attachment| attachment.identity)
        .unwrap();
    assert_ne!(candidate_identity, retry_identity);
    assert!(matches!(
        candidate.with_domain_attachment(&candidate_capability, |_| ()),
        Err(DomainAttachmentAccessError::StaleOrForeign)
    ));
    let failed = candidate.abort();
    assert_eq!(DOMAIN_A_STATS.counts(), (3, 3, 3));
    failed.close().unwrap();
}

#[test]
fn candidate_factory_failure_cleans_every_attachment_it_constructed() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    store.register_domain::<DomainA>().unwrap();
    store.register_domain::<DomainB>().unwrap();
    let failed = fail_store(store, &faults);

    FAIL_DOMAIN_B.store(true, Ordering::SeqCst);
    let failure = failed.recover_same_home().unwrap_err();
    assert!(matches!(
        failure.error(),
        beryl_home_store::HomeRecoveryError::Domain(
            DomainRegistrationError::AttachmentConstruction {
                domain: "runtime-b",
                ..
            }
        )
    ));
    assert_eq!(DOMAIN_A_STATS.counts(), (2, 2, 2));
    assert_eq!(DOMAIN_B_STATS.counts(), (1, 1, 1));

    FAIL_DOMAIN_B.store(false, Ordering::SeqCst);
    let failed = failure.into_store();
    let candidate = failed.recover_same_home().unwrap();
    assert_eq!(DOMAIN_A_STATS.counts(), (3, 2, 2));
    assert_eq!(DOMAIN_B_STATS.counts(), (2, 1, 1));
    let failed = candidate.abort();
    assert_eq!(DOMAIN_A_STATS.counts(), (3, 3, 3));
    assert_eq!(DOMAIN_B_STATS.counts(), (2, 2, 2));
    failed.close().unwrap();
}

#[test]
fn late_candidate_failure_retires_every_constructed_attachment_before_retry() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    store.register_domain::<DomainA>().unwrap();
    store.register_domain::<DomainB>().unwrap();
    let failed = fail_store(store, &faults);

    faults.fail_next(FaultPoint::AfterReopen);
    let failure = failed.recover_same_home().unwrap_err();
    assert!(matches!(
        failure.error(),
        beryl_home_store::HomeRecoveryError::Persistence { .. }
    ));
    assert_eq!(DOMAIN_A_STATS.counts(), (2, 2, 2));
    assert_eq!(DOMAIN_B_STATS.counts(), (2, 2, 2));

    let failed = failure.into_store();
    let candidate = failed.recover_same_home().unwrap();
    assert_eq!(DOMAIN_A_STATS.counts(), (3, 2, 2));
    assert_eq!(DOMAIN_B_STATS.counts(), (3, 2, 2));
    let failed = candidate.abort();
    assert_eq!(DOMAIN_A_STATS.counts(), (3, 3, 3));
    assert_eq!(DOMAIN_B_STATS.counts(), (3, 3, 3));
    failed.close().unwrap();
}

#[test]
fn attachment_type_mismatch_is_rejected_without_weakening_owner_identity() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);
    let handle = store.register_domain::<DomainA>().unwrap();
    let capability = handle.attachment_capability();
    let mismatched = capability_with_test_attachment_type::<DomainA, ()>(&capability);

    assert!(matches!(
        store.with_domain_attachment(&mismatched, |_| ()),
        Err(DomainAttachmentAccessError::AttachmentTypeMismatch {
            domain: "runtime-a"
        })
    ));
    assert!(store.with_domain_attachment(&capability, |_| ()).is_ok());
    store.close().unwrap();
    assert_eq!(DOMAIN_A_STATS.counts(), (1, 1, 1));
}

#[test]
fn stable_name_cannot_impersonate_the_registered_owner() {
    reset();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);
    store.register_domain::<DomainA>().unwrap();

    assert!(matches!(
        store.domain_handle::<ImpostorDomain>(),
        Err(DomainHandleError::OwnerTypeMismatch {
            domain: "runtime-a"
        })
    ));
    assert!(matches!(
        store.register_domain::<ImpostorDomain>(),
        Err(DomainRegistrationError::OwnerTypeMismatch {
            domain: "runtime-a"
        })
    ));
    assert_eq!(IMPOSTOR_STATS.counts(), (0, 0, 0));
    store.close().unwrap();
}
