use std::{
    error::Error,
    fmt,
    io::Write,
    marker::PhantomData,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use beryl_home_store::{
    CommandOutcome, DomainCallbackError, DomainCallbackSource, DomainHandle, DomainMutation,
    DomainReader, DomainReconciliation, DomainSchemaVersion, FirstAcceptancePromotionAdmission,
    FirstAcceptancePromotionAssetAdapter, FirstAcceptancePromotionAssetPlan,
    FirstAcceptancePromotionAssetSeed, FirstAcceptancePromotionObservation,
    FirstAcceptancePromotionSource, HomeCommand, HomeDomainRequirements, HomeOpenCandidate,
    HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuilder,
    ReadError, ReconciliationHandle, ReconciliationReader, ReconciliationReservation, RecordCodec,
    RecordFamily, RecordVersion, StorageDomain,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, FirstAcceptancePromotionSuccessorV1,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    SyndicDraftId, SyndicItemId,
};

pub static SOURCE_CALLS: AtomicUsize = AtomicUsize::new(0);
pub static ASSET_CALLS: AtomicUsize = AtomicUsize::new(0);
static FIXTURE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub enum TestError {
    Read(ReadError),
    Build(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(f),
            Self::Build(error) => error.fmt(f),
        }
    }
}

impl Error for TestError {}

impl DomainCallbackError for TestError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            other => Err(other),
        }
    }
}

impl From<ReadError> for TestError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

pub struct Source;
pub struct Asset;
pub struct Passive;
pub struct SourceRecord;
pub struct AssetRecord;
pub struct PassiveRecord;

macro_rules! domain {
    ($domain:ty, $record:ty, $name:literal) => {
        impl StorageDomain for $domain {
            const NAME: &'static str = $name;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] =
                &[RecordFamily::new::<$record>(KeyspaceSchemaVersion::new(1))];
            type ValidationError = TestError;
            type RuntimeAttachment = ();
            type RuntimeAttachmentError = std::convert::Infallible;

            fn create_runtime_attachment(
                _: &beryl_home_store::DomainRegistrationReader<'_, Self>,
            ) -> Result<(), Self::RuntimeAttachmentError> {
                Ok(())
            }

            fn validate(_: &DomainReader<'_, Self>) -> Result<(), TestError> {
                Ok(())
            }

            fn reconcile(
                reader: &ReconciliationReader<'_, Self>,
            ) -> Result<DomainReconciliation, TestError> {
                let mut count = 0;
                let mut old = true;
                let mut new = true;
                for record in reader.records::<$record>()? {
                    count += 1;
                    old &= record.current() == record.old();
                    new &= record.current() == record.new();
                }
                Ok(match (count, old, new) {
                    (0, _, _) => DomainReconciliation::Collision,
                    (_, true, true) => DomainReconciliation::ExactUnchanged,
                    (_, true, false) => DomainReconciliation::ExactOld,
                    (_, false, true) => DomainReconciliation::ExactNew,
                    _ => DomainReconciliation::Collision,
                })
            }
        }

        impl RecordCodec<$domain> for $record {
            type Key = u64;
            type Value = u64;
            type Error = std::io::Error;
            const FAMILY: &'static str = "records";
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = 8;
            const MAX_VALUE_BYTES: usize = 8;
            fn encode_key(value: &u64) -> Result<Vec<u8>, Self::Error> {
                Ok(value.to_be_bytes().to_vec())
            }
            fn decode_key(bytes: &[u8]) -> Result<u64, Self::Error> {
                decode(bytes)
            }
            fn encode_value(value: &u64) -> Result<Vec<u8>, Self::Error> {
                Ok(value.to_be_bytes().to_vec())
            }
            fn decode_value(bytes: &[u8]) -> Result<u64, Self::Error> {
                decode(bytes)
            }
        }
    };
}

fn decode(bytes: &[u8]) -> Result<u64, std::io::Error> {
    bytes
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid record"))
}

domain!(Source, SourceRecord, "unchanged-source");
domain!(Asset, AssetRecord, "unchanged-asset");
domain!(Passive, PassiveRecord, "unchanged-passive");

pub struct Edits<D, R> {
    records: Vec<(u64, Option<u64>)>,
    source: Option<FirstAcceptancePromotionAdmission>,
    asset: bool,
    _typed: PhantomData<fn(D, R)>,
}

impl<D, R> Edits<D, R> {
    pub fn new(records: &[(u64, Option<u64>)]) -> Self {
        Self {
            records: records.to_vec(),
            source: None,
            asset: false,
            _typed: PhantomData,
        }
    }
}

impl Edits<Source, SourceRecord> {
    pub fn promotion(value: u64, witness: bool) -> Self {
        let mut edits = Self::new(&[(1, Some(value))]);
        edits.source = Some(if witness {
            FirstAcceptancePromotionAdmission::AssetTransferRequired
        } else {
            FirstAcceptancePromotionAdmission::MarkerFree
        });
        edits
    }
}

impl Edits<Asset, AssetRecord> {
    pub fn promotion(value: u64) -> Self {
        let mut edits = Self::new(&[(1, Some(value))]);
        edits.asset = true;
        edits
    }
}

impl<D, R> DomainMutation<D> for Edits<D, R>
where
    D: StorageDomain<ValidationError = TestError>,
    R: RecordCodec<D, Key = u64, Value = u64>,
    SourceHook: FirstAcceptancePromotionSource<D>,
    AssetHook: FirstAcceptancePromotionAssetAdapter<D>,
    <<AssetHook as FirstAcceptancePromotionAssetAdapter<D>>::OwnerHead as RecordCodec<D>>::Value:
        Eq,
{
    type Error = TestError;
    type Prepared = Self;
    fn prepare(self, _: &DomainReader<'_, D>) -> Result<Self, TestError> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, D>,
    ) -> Result<(), TestError> {
        reservation
            .reserve_records::<R>(self.records.len().max(1))
            .map_err(build_error)?;
        if let Some(admission) = self.source {
            reservation
                .reserve_first_acceptance_promotion_source::<SourceHook>(admission)
                .map_err(build_error)?;
        }
        if self.asset {
            reservation
                .reserve_first_acceptance_promotion_asset::<AssetHook>(seed())
                .map_err(build_error)?;
        }
        Ok(())
    }
    fn contribute(prepared: Self, mutations: &mut MutationBuilder<'_, D>) -> Result<(), TestError> {
        for (key, value) in prepared.records {
            match value {
                Some(value) => mutations.put::<R>(&key, &value),
                None => mutations.delete::<R>(&key),
            }
            .map_err(build_error)?;
        }
        Ok(())
    }
}

fn build_error(error: impl Error + Send + Sync + 'static) -> TestError {
    TestError::Build(Box::new(error))
}

pub struct SourceHook;
pub struct AssetHook;

impl<D: StorageDomain<ValidationError = TestError>> FirstAcceptancePromotionSource<D>
    for SourceHook
{
    fn authenticate(
        _: &ReconciliationReader<'_, D>,
    ) -> Result<FirstAcceptancePromotionObservation, TestError> {
        SOURCE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(FirstAcceptancePromotionObservation::Authenticated(
            FirstAcceptancePromotionSuccessorV1::new(
                SyndicDraftId::from_bytes([1; 16]).accepted_input_id(),
                SyndicItemId::from_bytes([2; 16]),
                Some(proof()),
            ),
        ))
    }
}

macro_rules! asset_hook {
    ($domain:ty, $record:ty) => {
        impl FirstAcceptancePromotionAssetAdapter<$domain> for AssetHook {
            type OwnerHead = $record;
            const MAX_DECODED_BYTES: usize = 64;
            fn derive_plan(
                _: &FirstAcceptancePromotionAssetSeed,
                _: &FirstAcceptancePromotionSuccessorV1,
            ) -> Option<FirstAcceptancePromotionAssetPlan<u64, u64>> {
                ASSET_CALLS.fetch_add(1, Ordering::SeqCst);
                Some(FirstAcceptancePromotionAssetPlan {
                    original_draft: 11,
                    original_accepted: 12,
                    submitted: 13,
                    expected_submitted: 42,
                })
            }
        }
    };
}

asset_hook!(Source, SourceRecord);
asset_hook!(Asset, AssetRecord);
asset_hook!(Passive, PassiveRecord);

fn proof() -> SealedAssetReferenceSetProof {
    SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([3; 16]),
        SequentialMarkerSummaryV1::new([4; 32], 0, None).unwrap(),
        OrderedMarkerAssetSummaryV1::new([4; 32], 0),
        0,
        AssetReferenceSetDigest::from_bytes([4; 32]),
    )
    .unwrap()
}

fn seed() -> FirstAcceptancePromotionAssetSeed {
    let draft_id = SyndicDraftId::from_bytes([1; 16]);
    FirstAcceptancePromotionAssetSeed {
        draft_id,
        accepted_input_id: draft_id.accepted_input_id(),
        asset_reference_set: proof(),
    }
}

pub fn fixture(name: &str) -> PathBuf {
    let ledger = PathBuf::from(
        std::env::var_os("BERYL_UNCHANGED_FIXTURE_LEDGER")
            .expect("root must supply BERYL_UNCHANGED_FIXTURE_LEDGER"),
    );
    assert!(ledger.is_absolute());
    let path = ledger.parent().unwrap().join(format!(
        "{name}-{}-{}",
        std::process::id(),
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::SeqCst)
    ));
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&ledger)
        .unwrap();
    writeln!(file, "{}", path.display()).unwrap();
    file.sync_all().unwrap();
    std::fs::create_dir(&path).unwrap();
    path
}

pub struct Home {
    pub store: HomeStore,
    pub faults: FaultController,
    pub source: DomainHandle<Source>,
    pub asset: DomainHandle<Asset>,
    pub passive: DomainHandle<Passive>,
}

pub fn open(name: &str) -> Home {
    SOURCE_CALLS.store(0, Ordering::SeqCst);
    ASSET_CALLS.store(0, Ordering::SeqCst);
    let path = fixture(name);
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = candidate.register_domain::<Source>().unwrap();
    let asset = candidate.register_domain::<Asset>().unwrap();
    let passive = candidate.register_domain::<Passive>().unwrap();
    let store = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<Source>()
                .unwrap()
                .with_domain::<Asset>()
                .unwrap()
                .with_domain::<Passive>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    Home {
        store,
        faults,
        source,
        asset,
        passive,
    }
}

pub fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

pub fn installed(home: &Home, command: HomeCommand) -> ReconciliationHandle {
    home.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    match home.store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        outcome => panic!("expected indeterminate command: {outcome:?}"),
    }
}
