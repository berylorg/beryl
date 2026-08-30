#![cfg(feature = "test-faults")]

use std::{
    error::Error,
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use beryl_home_store::{
    test_faults::{FaultController, FaultPoint},
    CommandOutcome, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    DomainReconciliation, DomainSchemaVersion, HomeCommand, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuilder, PointReadLimit,
    ReadError, ReadStage, ReconciliationReader, ReconciliationReservation,
    ReconciliationResolution, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
};
use tempfile::tempdir;

static SERIAL: Mutex<()> = Mutex::new(());
static BLOCK_HOOKS: AtomicBool = AtomicBool::new(false);
static ACCESS_FAIL_HOOKS: AtomicBool = AtomicBool::new(false);
static STRUCTURAL_FAIL_HOOKS: AtomicBool = AtomicBool::new(false);
static ACTIVE_HOOKS: AtomicUsize = AtomicUsize::new(0);
static MAX_HOOKS: AtomicUsize = AtomicUsize::new(0);
static HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);
static VALIDATION_CALLS: AtomicUsize = AtomicUsize::new(0);
static RELEASE: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());

#[derive(Debug)]
enum TestError {
    Read(ReadError),
    Mutation(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::Mutation(error) => error.fmt(formatter),
        }
    }
}

impl Error for TestError {}

impl DomainCallbackError for TestError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            Self::Mutation(error) => Err(Self::Mutation(error)),
        }
    }
}

impl From<ReadError> for TestError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

struct Alpha;
struct Beta;
struct AlphaRecord;
struct BetaRecord;
struct CollisionDomain;
struct CollisionRecord;

macro_rules! codec {
    ($domain:ty, $codec:ty) => {
        impl RecordCodec<$domain> for $codec {
            type Key = u64;
            type Value = Vec<u8>;
            type Error = std::convert::Infallible;
            const FAMILY: &'static str = "records";
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = 8;
            const MAX_VALUE_BYTES: usize = 64;
            fn encode_key(key: &u64) -> Result<Vec<u8>, Self::Error> {
                Ok(key.to_be_bytes().to_vec())
            }
            fn decode_key(bytes: &[u8]) -> Result<u64, Self::Error> {
                Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
            }
            fn encode_value(value: &Vec<u8>) -> Result<Vec<u8>, Self::Error> {
                Ok(value.clone())
            }
            fn decode_value(bytes: &[u8]) -> Result<Vec<u8>, Self::Error> {
                Ok(bytes.to_vec())
            }
        }
    };
}

codec!(Alpha, AlphaRecord);
codec!(Beta, BetaRecord);

impl RecordCodec<CollisionDomain> for CollisionRecord {
    type Key = u64;
    type Value = Vec<u8>;
    type Error = std::convert::Infallible;
    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 8;
    const MAX_VALUE_BYTES: usize = 30 * 1024 * 1024;
    fn encode_key(key: &u64) -> Result<Vec<u8>, Self::Error> {
        Ok(key.to_be_bytes().to_vec())
    }
    fn decode_key(bytes: &[u8]) -> Result<u64, Self::Error> {
        Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
    }
    fn encode_value(value: &Vec<u8>) -> Result<Vec<u8>, Self::Error> {
        Ok(value.clone())
    }
    fn decode_value(bytes: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Ok(bytes.to_vec())
    }
}

struct HookActivity;

impl HookActivity {
    fn enter() -> Self {
        HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
        let active = ACTIVE_HOOKS.fetch_add(1, Ordering::SeqCst) + 1;
        MAX_HOOKS.fetch_max(active, Ordering::SeqCst);
        if BLOCK_HOOKS.load(Ordering::SeqCst) {
            let guard = RELEASE.0.lock().unwrap();
            drop(
                RELEASE
                    .1
                    .wait_while(guard, |_| BLOCK_HOOKS.load(Ordering::SeqCst))
                    .unwrap(),
            );
        }
        Self
    }
}

impl Drop for HookActivity {
    fn drop(&mut self) {
        ACTIVE_HOOKS.fetch_sub(1, Ordering::SeqCst);
    }
}

fn classify<D, R>(reader: &ReconciliationReader<'_, D>) -> Result<DomainReconciliation, TestError>
where
    D: StorageDomain<ValidationError = TestError>,
    R: RecordCodec<D, Key = u64, Value = Vec<u8>>,
{
    let _activity = HookActivity::enter();
    if ACCESS_FAIL_HOOKS.load(Ordering::SeqCst) {
        return Err(TestError::Read(ReadError::Storage {
            stage: ReadStage::PointValue,
            source: Box::new(std::io::Error::other("typed reconciliation access failure")),
        }));
    }
    if STRUCTURAL_FAIL_HOOKS.load(Ordering::SeqCst) {
        return Err(TestError::Read(ReadError::MalformedRecord {
            domain: D::NAME,
            family: R::FAMILY,
        }));
    }
    let mut side = None;
    for record in reader.records::<R>()? {
        let record_side = if record.current() == record.old() {
            DomainReconciliation::ExactOld
        } else if record.current() == record.new() {
            DomainReconciliation::ExactNew
        } else {
            DomainReconciliation::Collision
        };
        if side.is_some_and(|side| side != record_side) {
            return Ok(DomainReconciliation::Collision);
        }
        side = Some(record_side);
    }
    Ok(side.unwrap_or(DomainReconciliation::Collision))
}

macro_rules! domain {
    ($domain:ty, $codec:ty, $name:literal) => {
        impl StorageDomain for $domain {
            const NAME: &'static str = $name;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] =
                &[RecordFamily::new::<$codec>(KeyspaceSchemaVersion::new(1))];
            type ValidationError = TestError;
            type RuntimeAttachment = ();
            type RuntimeAttachmentError = std::convert::Infallible;

            fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
                Ok(())
            }

            fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                VALIDATION_CALLS.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            fn reconcile(
                reader: &ReconciliationReader<'_, Self>,
            ) -> Result<DomainReconciliation, Self::ValidationError> {
                classify::<Self, $codec>(reader)
            }
        }
    };
}

domain!(Alpha, AlphaRecord, "target_alpha");
domain!(Beta, BetaRecord, "target_beta");

impl StorageDomain for CollisionDomain {
    const NAME: &'static str = "target_collision";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<CollisionRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = TestError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
    fn reconcile(
        reader: &ReconciliationReader<'_, Self>,
    ) -> Result<DomainReconciliation, Self::ValidationError> {
        let _activity = HookActivity::enter();
        let records = reader.records::<CollisionRecord>()?;
        assert_eq!(records.len(), 1);
        Ok(DomainReconciliation::Collision)
    }
}

struct Put<D, R> {
    key: u64,
    value: Vec<u8>,
    _typed: std::marker::PhantomData<fn(D, R)>,
}

impl<D, R> Put<D, R> {
    fn new(key: u64, value: &[u8]) -> Self {
        Self {
            key,
            value: value.to_vec(),
            _typed: std::marker::PhantomData,
        }
    }
}

impl<D, R> DomainMutation<D> for Put<D, R>
where
    D: StorageDomain<ValidationError = TestError>,
    R: RecordCodec<D, Key = u64, Value = Vec<u8>>,
{
    type Error = TestError;
    type Prepared = Self;
    fn prepare(self, _reader: &DomainReader<'_, D>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, D>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<R>(1)
            .map_err(|error| TestError::Mutation(Box::new(error)))
    }
    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<R>(&prepared.key, &prepared.value)
            .map_err(|error| TestError::Mutation(Box::new(error)))
    }
}

fn reset_counters() {
    BLOCK_HOOKS.store(false, Ordering::SeqCst);
    ACCESS_FAIL_HOOKS.store(false, Ordering::SeqCst);
    STRUCTURAL_FAIL_HOOKS.store(false, Ordering::SeqCst);
    ACTIVE_HOOKS.store(0, Ordering::SeqCst);
    MAX_HOOKS.store(0, Ordering::SeqCst);
    HOOK_CALLS.store(0, Ordering::SeqCst);
    VALIDATION_CALLS.store(0, Ordering::SeqCst);
}

fn wait_for_active(expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while ACTIVE_HOOKS.load(Ordering::SeqCst) != expected {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected} active hooks"
        );
        thread::yield_now();
    }
}

#[test]
fn exact_new_reconstructs_receipt_and_releases_scope_without_validation() {
    let _serial = SERIAL.lock().unwrap();
    reset_counters();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = store.register_domain::<Alpha>().unwrap();
    let validation_before = VALIDATION_CALLS.load(Ordering::SeqCst);

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store
        .execute_current(alpha.current_command(Put::<Alpha, AlphaRecord>::new(7, b"new")))
    {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    assert_eq!(store.pending_reconciliations().len(), 1);
    let receipt = match store.reconcile(&handle).unwrap() {
        ReconciliationResolution::ExactNew { receipt } => receipt,
        other => panic!("expected exact-new, got {other:?}"),
    };
    assert_eq!(receipt.home_revision().get(), 2);
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(VALIDATION_CALLS.load(Ordering::SeqCst), validation_before);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    store.close().unwrap();
}

#[test]
fn exact_old_after_reopen_releases_the_gate_without_fabricating_a_receipt() {
    let _serial = SERIAL.lock().unwrap();
    reset_counters();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = store.register_domain::<Alpha>().unwrap();

    let fault = fjall::test_faults::fail_next_journal_write();
    let handle = match store
        .execute_current(alpha.current_command(Put::<Alpha, AlphaRecord>::new(8, b"not published")))
    {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    drop(fault);
    assert_eq!(store.home_revision().unwrap().get(), 1);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    let store = store.recover_same_home().unwrap().publish();
    let validation_before = VALIDATION_CALLS.load(Ordering::SeqCst);

    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactOld
    );
    assert_eq!(VALIDATION_CALLS.load(Ordering::SeqCst), validation_before);
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    store.close().unwrap();
}

#[test]
fn mixed_and_neither_observations_seal_collision_without_health_failure() {
    let _serial = SERIAL.lock().unwrap();
    reset_counters();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = store.register_domain::<Alpha>().unwrap();
    let beta = store.register_domain::<Beta>().unwrap();

    let mut seed = HomeCommand::new(store.home_revision().unwrap());
    seed.add(alpha.contribution(
        store.domain_revision(&alpha).unwrap(),
        Put::<Alpha, AlphaRecord>::new(1, b"old"),
    ))
    .unwrap();
    seed.add(beta.contribution(
        store.domain_revision(&beta).unwrap(),
        Put::<Beta, BetaRecord>::new(1, b"old"),
    ))
    .unwrap();
    assert!(matches!(
        store.execute(seed),
        CommandOutcome::Committed { .. }
    ));

    let mut ambiguous = HomeCommand::new(store.home_revision().unwrap());
    ambiguous
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            Put::<Alpha, AlphaRecord>::new(1, b"new"),
        ))
        .unwrap();
    ambiguous
        .add(beta.contribution(
            store.domain_revision(&beta).unwrap(),
            Put::<Beta, BetaRecord>::new(1, b"new"),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let mixed = match store.execute(ambiguous) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    assert!(matches!(
        store.execute_current(alpha.current_command(Put::<Alpha, AlphaRecord>::new(1, b"old"))),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(
        store.reconcile(&mixed).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        store.reconcile(&mixed).unwrap(),
        ReconciliationResolution::Collision
    );

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let neither = match store
        .execute_current(alpha.current_command(Put::<Alpha, AlphaRecord>::new(2, b"intended")))
    {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    assert!(matches!(
        store.execute_current(alpha.current_command(Put::<Alpha, AlphaRecord>::new(2, b"third"))),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(
        store.reconcile(&neither).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    store.close().unwrap();
}

#[test]
fn collision_custody_survives_drop_but_orderly_close_releases_ownership() {
    let _serial = SERIAL.lock().unwrap();
    reset_counters();

    let abandoned_directory = tempdir().unwrap();
    let abandoned_faults = FaultController::new();
    let mut abandoned = HomeStore::open_with_faults(
        HomeOpenOptions::new(abandoned_directory.path(), HomeSchemaVersion::CURRENT),
        abandoned_faults.clone(),
    )
    .unwrap();
    let abandoned_alpha = abandoned.register_domain::<Alpha>().unwrap();
    abandoned_faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let abandoned_handle =
        match abandoned.execute_current(
            abandoned_alpha.current_command(Put::<Alpha, AlphaRecord>::new(21, b"intended")),
        ) {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                reconciliation.install_and_handle()
            }
            other => panic!("expected indeterminate outcome, got {other:?}"),
        };
    assert!(matches!(
        abandoned.execute_current(
            abandoned_alpha.current_command(Put::<Alpha, AlphaRecord>::new(21, b"third")),
        ),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(
        abandoned.reconcile(&abandoned_handle).unwrap(),
        ReconciliationResolution::Collision
    );
    drop(abandoned);
    drop(abandoned_handle);
    assert!(matches!(
        HomeStore::open(HomeOpenOptions::new(
            abandoned_directory.path(),
            HomeSchemaVersion::CURRENT,
        )),
        Err(beryl_home_store::HomeOpenError::Busy { .. })
    ));

    let orderly_directory = tempdir().unwrap();
    let orderly_faults = FaultController::new();
    let mut orderly = HomeStore::open_with_faults(
        HomeOpenOptions::new(orderly_directory.path(), HomeSchemaVersion::CURRENT),
        orderly_faults.clone(),
    )
    .unwrap();
    let orderly_alpha = orderly.register_domain::<Alpha>().unwrap();
    orderly_faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let orderly_handle =
        match orderly.execute_current(
            orderly_alpha.current_command(Put::<Alpha, AlphaRecord>::new(22, b"intended")),
        ) {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                reconciliation.install_and_handle()
            }
            other => panic!("expected indeterminate outcome, got {other:?}"),
        };
    assert!(matches!(
        orderly.execute_current(
            orderly_alpha.current_command(Put::<Alpha, AlphaRecord>::new(22, b"third")),
        ),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(
        orderly.reconcile(&orderly_handle).unwrap(),
        ReconciliationResolution::Collision
    );
    orderly.close().unwrap();
    drop(orderly_handle);

    HomeStore::open(HomeOpenOptions::new(
        orderly_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap()
    .close()
    .unwrap();
}

#[test]
fn duplicate_trigger_joins_and_four_workers_leave_unrelated_work_available() {
    let _serial = SERIAL.lock().unwrap();
    reset_counters();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = store.register_domain::<Alpha>().unwrap();
    let beta = store.register_domain::<Beta>().unwrap();
    let mut handles = Vec::new();
    for key in 0..5 {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let handle = match store
            .execute_current(alpha.current_command(Put::<Alpha, AlphaRecord>::new(key, b"new")))
        {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                reconciliation.install_and_handle()
            }
            other => panic!("expected indeterminate outcome, got {other:?}"),
        };
        handles.push(handle);
    }

    BLOCK_HOOKS.store(true, Ordering::SeqCst);
    let store = Arc::new(store);
    let mut workers = Vec::new();
    for handle in handles.iter().cloned() {
        let store = Arc::clone(&store);
        workers.push(thread::spawn(move || store.reconcile(&handle)));
    }
    let joined = handles[0].clone();
    let joined_store = Arc::clone(&store);
    workers.push(thread::spawn(move || joined_store.reconcile(&joined)));
    wait_for_active(4);
    thread::sleep(Duration::from_millis(50));
    assert_eq!(ACTIVE_HOOKS.load(Ordering::SeqCst), 4);
    assert_eq!(MAX_HOOKS.load(Ordering::SeqCst), 4);
    assert_eq!(HOOK_CALLS.load(Ordering::SeqCst), 4);

    assert_eq!(
        store
            .read_point::<Beta, BetaRecord>(&beta, &99, PointReadLimit::new(128).unwrap(),)
            .unwrap(),
        None
    );
    assert!(matches!(
        store.execute_current(beta.current_command(Put::<Beta, BetaRecord>::new(99, b"healthy"))),
        CommandOutcome::Committed { .. }
    ));

    BLOCK_HOOKS.store(false, Ordering::SeqCst);
    RELEASE.1.notify_all();
    for worker in workers {
        assert!(matches!(
            worker.join().unwrap().unwrap(),
            ReconciliationResolution::ExactNew { .. }
        ));
    }
    assert_eq!(HOOK_CALLS.load(Ordering::SeqCst), 5);
    assert_eq!(MAX_HOOKS.load(Ordering::SeqCst), 4);
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    Arc::try_unwrap(store).ok().unwrap().close().unwrap();
}

#[test]
fn collision_replaces_the_conservative_descriptor_charge() {
    let _serial = SERIAL.lock().unwrap();
    reset_counters();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let domain = store.register_domain::<CollisionDomain>().unwrap();
    let mut handles = Vec::new();
    for key in 0..4 {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        match store.execute_current(
            domain.current_command(Put::<CollisionDomain, CollisionRecord>::new(key, b"value")),
        ) {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                handles.push(reconciliation.install_and_handle());
            }
            other => panic!("expected indeterminate outcome, got {other:?}"),
        }
    }
    assert!(matches!(
        store.execute_current(
            domain.current_command(Put::<CollisionDomain, CollisionRecord>::new(4, b"blocked")),
        ),
        CommandOutcome::NotCommitted {
            evidence: beryl_home_store::CommandError::ReconciliationCapacity,
        }
    ));

    assert_eq!(
        store.reconcile(&handles[0]).unwrap(),
        ReconciliationResolution::Collision
    );
    assert!(matches!(
        store.execute_current(
            domain.current_command(Put::<CollisionDomain, CollisionRecord>::new(4, b"admitted")),
        ),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    let close = store.close().unwrap_err();
    assert_eq!(close.pending_reconciliation_scopes(), Some(3));
    let store = close.into_open_store().unwrap();
    for handle in &handles[1..] {
        assert_eq!(
            store.reconcile(handle).unwrap(),
            ReconciliationResolution::Collision
        );
    }
    store.close().unwrap();
}

#[test]
fn nonstructural_hook_access_failure_is_scope_local_joined_and_retryable() {
    let _serial = SERIAL.lock().unwrap();
    reset_counters();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = store.register_domain::<Alpha>().unwrap();
    let beta = store.register_domain::<Beta>().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store
        .execute_current(alpha.current_command(Put::<Alpha, AlphaRecord>::new(12, b"new")))
    {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };

    ACCESS_FAIL_HOOKS.store(true, Ordering::SeqCst);
    let first = store.reconcile(&handle).unwrap_err().to_string();
    let second = store.reconcile(&handle).unwrap_err().to_string();
    assert_eq!(first, second);
    assert_eq!(HOOK_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(
        store
            .read_point::<Beta, BetaRecord>(&beta, &77, PointReadLimit::new(128).unwrap(),)
            .unwrap(),
        None
    );
    assert!(matches!(
        store
            .execute_current(beta.current_command(Put::<Beta, BetaRecord>::new(77, b"unrelated")),),
        CommandOutcome::Committed { .. }
    ));
    let close = store.close().unwrap_err();
    assert_eq!(close.pending_reconciliation_scopes(), Some(1));
    let store = close.into_open_store().unwrap();

    // Enumeration after a completed typed failure creates a fresh trigger flight while retaining
    // the same sole descriptor, slot, and conservative byte charge.
    let retry = store.pending_reconciliations().pop().unwrap();
    ACCESS_FAIL_HOOKS.store(false, Ordering::SeqCst);
    assert!(matches!(
        store.reconcile(&retry).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(HOOK_CALLS.load(Ordering::SeqCst), 2);
    store.close().unwrap();
}

#[test]
fn structural_hook_access_evidence_fails_health_and_retains_scope() {
    let _serial = SERIAL.lock().unwrap();
    reset_counters();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = store.register_domain::<Alpha>().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store
        .execute_current(alpha.current_command(Put::<Alpha, AlphaRecord>::new(13, b"new")))
    {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };

    STRUCTURAL_FAIL_HOOKS.store(true, Ordering::SeqCst);
    assert!(store.reconcile(&handle).is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert_eq!(store.pending_reconciliations().len(), 1);

    let store = store.recover_same_home().unwrap().publish();
    STRUCTURAL_FAIL_HOOKS.store(false, Ordering::SeqCst);
    let retry = store.pending_reconciliations().pop().unwrap();
    assert!(matches!(
        store.reconcile(&retry).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    store.close().unwrap();
}
