use std::{
    error::Error,
    fmt,
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, DomainCallbackError, DomainCallbackSource,
    DomainMutation, DomainReader, DomainReconciliation, DomainSchemaVersion,
    FirstAcceptancePromotionAdmission, FirstAcceptancePromotionAssetAdapter,
    FirstAcceptancePromotionAssetPlan, FirstAcceptancePromotionAssetSeed,
    FirstAcceptancePromotionObservation, FirstAcceptancePromotionSource, HomeCommand,
    HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuilder,
    ReadError, ReconciliationReader, ReconciliationReservation, ReconciliationResolution,
    RecordCodec, RecordFamily, RecordVersion, StorageDomain,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, FirstAcceptancePromotionSuccessorV1,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    SyndicAcceptedInputId, SyndicDraftId, SyndicItemId,
};
use tempfile::tempdir;

const SOURCE_KEY: u64 = 1;
const ORDINARY_ASSET_KEY: u64 = 7;
const PASSIVE_KEY: u64 = 9;
const ORIGINAL_DRAFT_KEY: u64 = 11;
const ORIGINAL_ACCEPTED_KEY: u64 = 12;
const SUBMITTED_KEY: u64 = 13;
const INVALID_KEY: u64 = u64::MAX;
const OVERSIZED_KEY: u64 = u64::MAX - 1;

static SOURCE_CALLS: AtomicUsize = AtomicUsize::new(0);
static BLOCK_SOURCE: AtomicBool = AtomicBool::new(false);
static RELEASE_SOURCE: AtomicBool = AtomicBool::new(false);
static FAIL_SOURCE: AtomicBool = AtomicBool::new(false);
static SERIAL: Mutex<()> = Mutex::new(());

#[derive(Debug)]
enum TestError {
    Read(ReadError),
    Build(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::Build(error) => error.fmt(formatter),
        }
    }
}

impl Error for TestError {}

impl DomainCallbackError for TestError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            Self::Build(error) => Err(Self::Build(error)),
        }
    }
}

impl From<ReadError> for TestError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

struct SourceDomain;
struct AssetDomain;
struct PassiveDomain;
struct SourceRecord;
struct AssetRecord;
struct ForeignAssetRecord;
struct LargeAssetRecord;
struct TooLargeAssetRecord;
struct PassiveRecord;

macro_rules! codec {
    ($domain:ty, $record:ty, $family:literal, $maximum:expr) => {
        impl RecordCodec<$domain> for $record {
            type Key = u64;
            type Value = Vec<u8>;
            type Error = std::io::Error;
            const FAMILY: &'static str = $family;
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = 8;
            const MAX_VALUE_BYTES: usize = $maximum;
            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                if *key == INVALID_KEY {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "invalid fixed derived key",
                    ));
                }
                if *key == OVERSIZED_KEY {
                    return Ok(vec![0; Self::MAX_KEY_BYTES + 1]);
                }
                Ok(key.to_be_bytes().to_vec())
            }
            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                Ok(u64::from_be_bytes(encoded.try_into().map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid key")
                })?))
            }
            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
                if value.as_slice() == [0xfe] {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "invalid fixed expected value",
                    ));
                }
                Ok(value.clone())
            }
            fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
                Ok(encoded.to_vec())
            }
        }
    };
}

codec!(SourceDomain, SourceRecord, "source", 32);
codec!(AssetDomain, AssetRecord, "asset", 32);
codec!(AssetDomain, ForeignAssetRecord, "foreign-asset", 32);
codec!(
    AssetDomain,
    LargeAssetRecord,
    "large-asset",
    12 * 1024 * 1024
);
codec!(
    AssetDomain,
    TooLargeAssetRecord,
    "too-large-asset",
    22 * 1024 * 1024
);
codec!(PassiveDomain, PassiveRecord, "passive", 32);

fn classify<D, R>(reader: &ReconciliationReader<'_, D>) -> Result<DomainReconciliation, TestError>
where
    D: StorageDomain<ValidationError = TestError>,
    R: RecordCodec<D, Key = u64, Value = Vec<u8>>,
{
    let mut side = None;
    for record in reader.records::<R>()? {
        let current = if record.current() == record.old() {
            DomainReconciliation::ExactOld
        } else if record.current() == record.new() {
            DomainReconciliation::ExactNew
        } else {
            DomainReconciliation::Collision
        };
        if side.is_some_and(|existing| existing != current) {
            return Ok(DomainReconciliation::Collision);
        }
        side = Some(current);
    }
    Ok(side.unwrap_or(DomainReconciliation::Collision))
}

macro_rules! domain {
    ($domain:ty, $name:literal, $record:ty, $families:expr) => {
        impl StorageDomain for $domain {
            const NAME: &'static str = $name;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] = $families;
            type ValidationError = TestError;
            type RuntimeAttachment = ();
            type RuntimeAttachmentError = std::convert::Infallible;
            fn create_runtime_attachment(
                _: &beryl_home_store::DomainRegistrationReader<'_, Self>,
            ) -> Result<(), Self::RuntimeAttachmentError> {
                Ok(())
            }
            fn validate(_: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                Ok(())
            }
            fn reconcile(
                reader: &ReconciliationReader<'_, Self>,
            ) -> Result<DomainReconciliation, Self::ValidationError> {
                classify::<Self, $record>(reader)
            }
        }
    };
}

domain!(
    SourceDomain,
    "first-acceptance-source",
    SourceRecord,
    &[RecordFamily::new::<SourceRecord>(
        KeyspaceSchemaVersion::new(1)
    )]
);
domain!(
    AssetDomain,
    "first-acceptance-asset",
    AssetRecord,
    &[
        RecordFamily::new::<AssetRecord>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<LargeAssetRecord>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<TooLargeAssetRecord>(KeyspaceSchemaVersion::new(1)),
    ]
);
domain!(
    PassiveDomain,
    "first-acceptance-passive",
    PassiveRecord,
    &[RecordFamily::new::<PassiveRecord>(
        KeyspaceSchemaVersion::new(1)
    )]
);

struct Put<D, R> {
    key: u64,
    value: Vec<u8>,
    _typed: PhantomData<fn(D, R)>,
}
impl<D, R> Put<D, R> {
    fn new(key: u64, value: u64) -> Self {
        Self {
            key,
            value: value.to_be_bytes().to_vec(),
            _typed: PhantomData,
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
    fn prepare(self, _: &DomainReader<'_, D>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, D>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<R>(1)
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<R>(&prepared.key, &prepared.value)
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}

struct SourcePromotion<S> {
    admission: FirstAcceptancePromotionAdmission,
    duplicate: bool,
    _source: PhantomData<S>,
}
impl<S> SourcePromotion<S> {
    fn new(admission: FirstAcceptancePromotionAdmission) -> Self {
        Self {
            admission,
            duplicate: false,
            _source: PhantomData,
        }
    }
}
impl<S> DomainMutation<SourceDomain> for SourcePromotion<S>
where
    S: FirstAcceptancePromotionSource<SourceDomain>,
{
    type Error = TestError;
    type Prepared = Self;
    fn prepare(self, _: &DomainReader<'_, SourceDomain>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SourceDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<SourceRecord>(1)
            .and_then(|()| {
                reservation.reserve_first_acceptance_promotion_source::<S>(self.admission)
            })
            .and_then(|()| {
                if self.duplicate {
                    reservation.reserve_first_acceptance_promotion_source::<S>(self.admission)
                } else {
                    Ok(())
                }
            })
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        _: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SourceDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<SourceRecord>(&SOURCE_KEY, &2_u64.to_be_bytes().to_vec())
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}

struct AssetPromotion<A, R> {
    seed: FirstAcceptancePromotionAssetSeed,
    _adapter: PhantomData<(A, R)>,
}
impl<A, R> AssetPromotion<A, R> {
    fn new(seed: FirstAcceptancePromotionAssetSeed) -> Self {
        Self {
            seed,
            _adapter: PhantomData,
        }
    }
}
impl<A, R> DomainMutation<AssetDomain> for AssetPromotion<A, R>
where
    A: FirstAcceptancePromotionAssetAdapter<AssetDomain, OwnerHead = R>,
    R: RecordCodec<AssetDomain, Key = u64, Value = Vec<u8>>,
    R::Value: Eq,
{
    type Error = TestError;
    type Prepared = Self;
    fn prepare(self, _: &DomainReader<'_, AssetDomain>) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<R>(1)
            .and_then(|()| reservation.reserve_first_acceptance_promotion_asset::<A>(self.seed))
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        _: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<R>(&ORDINARY_ASSET_KEY, &2_u64.to_be_bytes().to_vec())
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}

fn draft() -> SyndicDraftId {
    SyndicDraftId::from_bytes([41; 16])
}
fn accepted() -> SyndicAcceptedInputId {
    draft().accepted_input_id()
}
fn submitted() -> SyndicItemId {
    SyndicItemId::from_bytes([42; 16])
}
fn proof_with(set_seed: u8, summary_seed: u8) -> SealedAssetReferenceSetProof {
    SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([set_seed; 16]),
        SequentialMarkerSummaryV1::new([summary_seed; 32], 0, None).unwrap(),
        OrderedMarkerAssetSummaryV1::new([summary_seed; 32], 0),
        0,
        AssetReferenceSetDigest::from_bytes([summary_seed; 32]),
    )
    .unwrap()
}

fn proof(seed: u8) -> SealedAssetReferenceSetProof {
    proof_with(seed, seed)
}
fn seed(proof: SealedAssetReferenceSetProof) -> FirstAcceptancePromotionAssetSeed {
    FirstAcceptancePromotionAssetSeed {
        draft_id: draft(),
        accepted_input_id: accepted(),
        asset_reference_set: proof,
    }
}
fn expected_submitted() -> Vec<u8> {
    42_u64.to_be_bytes().to_vec()
}
fn asset_plan(
    seed: &FirstAcceptancePromotionAssetSeed,
    correlation: &FirstAcceptancePromotionSuccessorV1,
    draft_key: u64,
) -> Option<FirstAcceptancePromotionAssetPlan<u64, Vec<u8>>> {
    (correlation.accepted_input_id() == seed.accepted_input_id
        && correlation.asset_reference_set() == Some(seed.asset_reference_set))
    .then_some(FirstAcceptancePromotionAssetPlan {
        original_draft: draft_key,
        original_accepted: ORIGINAL_ACCEPTED_KEY,
        submitted: SUBMITTED_KEY,
        expected_submitted: expected_submitted(),
    })
}

struct MarkerSource;
struct AssetSource;
struct WrongAssetSource;
struct BlockingMarkerSource;
struct FailingMarkerSource;
fn authenticate(
    reader: &ReconciliationReader<'_, SourceDomain>,
    asset_proof: Option<SealedAssetReferenceSetProof>,
) -> Result<FirstAcceptancePromotionObservation, TestError> {
    SOURCE_CALLS.fetch_add(1, Ordering::SeqCst);
    while BLOCK_SOURCE.load(Ordering::SeqCst) && !RELEASE_SOURCE.load(Ordering::SeqCst) {
        thread::yield_now();
    }
    if FAIL_SOURCE.load(Ordering::SeqCst) {
        return Err(TestError::Read(ReadError::Storage {
            stage: beryl_home_store::ReadStage::PointValue,
            source: Box::new(std::io::Error::other("source failure")),
        }));
    }
    let records = reader.records::<SourceRecord>()?;
    let current = records.first().and_then(|record| record.current());
    let expected = expected_submitted();
    if current != Some(&expected) {
        return Ok(FirstAcceptancePromotionObservation::Unresolved);
    }
    Ok(FirstAcceptancePromotionObservation::Authenticated(
        FirstAcceptancePromotionSuccessorV1::new(accepted(), submitted(), asset_proof),
    ))
}
macro_rules! source {
    ($source:ty, $value:expr) => {
        impl FirstAcceptancePromotionSource<SourceDomain> for $source {
            fn authenticate(
                reader: &ReconciliationReader<'_, SourceDomain>,
            ) -> Result<FirstAcceptancePromotionObservation, TestError> {
                authenticate(reader, $value)
            }
        }
    };
}
source!(MarkerSource, None);
source!(AssetSource, Some(proof(1)));
source!(WrongAssetSource, Some(proof_with(1, 2)));
source!(BlockingMarkerSource, None);
source!(FailingMarkerSource, None);

struct NormalAsset;
struct InvalidKeyAsset;
struct OversizedKeyAsset;
struct DecodedLimitAsset;
struct RejectedAsset;
struct CurrentRejectedAsset;
struct InvalidExpectedAsset;
struct DuplicateInvalidAsset;
struct ForeignAsset;
struct LargeAsset;
struct TooLargeAsset;
macro_rules! asset {
    ($adapter:ty, $record:ty, $limit:expr, $draft:expr) => {
        impl FirstAcceptancePromotionAssetAdapter<AssetDomain> for $adapter {
            type OwnerHead = $record;
            const MAX_DECODED_BYTES: usize = $limit;
            fn derive_plan(
                seed: &FirstAcceptancePromotionAssetSeed,
                correlation: &FirstAcceptancePromotionSuccessorV1,
            ) -> Option<FirstAcceptancePromotionAssetPlan<u64, Vec<u8>>> {
                asset_plan(seed, correlation, $draft)
            }
        }
    };
}
asset!(NormalAsset, AssetRecord, 64, ORIGINAL_DRAFT_KEY);
asset!(InvalidKeyAsset, AssetRecord, 64, INVALID_KEY);
asset!(OversizedKeyAsset, AssetRecord, 64, OVERSIZED_KEY);
asset!(DecodedLimitAsset, AssetRecord, 16, ORIGINAL_DRAFT_KEY);
asset!(RejectedAsset, AssetRecord, 15, ORIGINAL_DRAFT_KEY);
asset!(CurrentRejectedAsset, AssetRecord, 16, ORIGINAL_DRAFT_KEY);
impl FirstAcceptancePromotionAssetAdapter<AssetDomain> for InvalidExpectedAsset {
    type OwnerHead = AssetRecord;
    const MAX_DECODED_BYTES: usize = 64;

    fn derive_plan(
        seed: &FirstAcceptancePromotionAssetSeed,
        correlation: &FirstAcceptancePromotionSuccessorV1,
    ) -> Option<FirstAcceptancePromotionAssetPlan<u64, Vec<u8>>> {
        asset_plan(seed, correlation, ORIGINAL_DRAFT_KEY).map(|mut plan| {
            plan.expected_submitted = vec![0xfe];
            plan
        })
    }
}
impl FirstAcceptancePromotionAssetAdapter<AssetDomain> for DuplicateInvalidAsset {
    type OwnerHead = AssetRecord;
    const MAX_DECODED_BYTES: usize = 64;

    fn derive_plan(
        seed: &FirstAcceptancePromotionAssetSeed,
        correlation: &FirstAcceptancePromotionSuccessorV1,
    ) -> Option<FirstAcceptancePromotionAssetPlan<u64, Vec<u8>>> {
        asset_plan(seed, correlation, ORIGINAL_DRAFT_KEY).map(|mut plan| {
            plan.original_accepted = ORIGINAL_DRAFT_KEY;
            plan.expected_submitted = vec![0xfe];
            plan
        })
    }
}
asset!(ForeignAsset, ForeignAssetRecord, 64, ORIGINAL_DRAFT_KEY);
asset!(LargeAsset, LargeAssetRecord, 64, ORIGINAL_DRAFT_KEY);
asset!(TooLargeAsset, TooLargeAssetRecord, 64, ORIGINAL_DRAFT_KEY);

fn reset_hooks() {
    SOURCE_CALLS.store(0, Ordering::SeqCst);
    BLOCK_SOURCE.store(false, Ordering::SeqCst);
    RELEASE_SOURCE.store(false, Ordering::SeqCst);
    FAIL_SOURCE.store(false, Ordering::SeqCst);
}
fn committed(outcome: CommandOutcome) {
    assert!(matches!(outcome, CommandOutcome::Committed { .. }));
}
fn installed(outcome: CommandOutcome) -> beryl_home_store::ReconciliationHandle {
    match outcome {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    }
}
fn open() -> (tempfile::TempDir, FaultController, HomeStore) {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    (directory, faults, store)
}

mod flight_capacity;
mod ordinary_source;
mod witness_adversarial;
