use std::{
    error::Error,
    fmt,
    io::Write,
    num::NonZeroU64,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use beryl_home_store::{
    CommandOutcome, DomainCallbackError, DomainCallbackSource, DomainHandle, DomainMutation,
    DomainReader, DomainReconciliation, DomainSchemaVersion, HomeCommand, HomeOpenCandidate,
    HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuilder,
    ReadError, ReconciliationReader, ReconciliationReservation, RecordCodec, RecordFamily,
    RecordVersion, SidecarByteLimit, SidecarNamespace, StorageDomain, test_faults::FaultController,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, ImageLabelOrdinal, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof, SequentialMarkerSummaryV1, SyndicDraftMarkerId,
    advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetReferencePageEntry,
    AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState, PublishAssetMetadata,
    SealAssetReferenceSet,
};

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

pub struct Probe;
pub struct ProbeRecord;

impl StorageDomain for Probe {
    const NAME: &'static str = "asset-reconciliation-probe";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<ProbeRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
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
    ) -> Result<DomainReconciliation, TestError> {
        let records = reader.records::<ProbeRecord>()?;
        let old = records
            .iter()
            .all(|record| record.current() == record.old());
        let new = records
            .iter()
            .all(|record| record.current() == record.new());
        Ok(match (records.is_empty(), old, new) {
            (false, true, false) => DomainReconciliation::ExactOld,
            (false, false, true) => DomainReconciliation::ExactNew,
            (false, true, true) => DomainReconciliation::ExactUnchanged,
            _ => DomainReconciliation::Collision,
        })
    }
}

impl RecordCodec<Probe> for ProbeRecord {
    type Key = u8;
    type Value = u8;
    type Error = std::convert::Infallible;
    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 1;
    fn encode_key(value: &u8) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*value])
    }
    fn decode_key(bytes: &[u8]) -> Result<u8, Self::Error> {
        Ok(bytes[0])
    }
    fn encode_value(value: &u8) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*value])
    }
    fn decode_value(bytes: &[u8]) -> Result<u8, Self::Error> {
        Ok(bytes[0])
    }
}

pub struct PutProbe(pub u8);

impl DomainMutation<Probe> for PutProbe {
    type Error = TestError;
    type Prepared = Self;
    fn prepare(self, _: &DomainReader<'_, Probe>) -> Result<Self, TestError> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, Probe>,
    ) -> Result<(), TestError> {
        reservation
            .reserve_records::<ProbeRecord>(1)
            .map_err(|error| TestError::Build(Box::new(error)))
    }
    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, Probe>,
    ) -> Result<(), TestError> {
        mutations
            .put::<ProbeRecord>(&1, &prepared.0)
            .map_err(|error| TestError::Build(Box::new(error)))
    }
}

pub struct Home {
    pub path: PathBuf,
    pub store: HomeStore,
    pub state: BerylState,
    pub probe: DomainHandle<Probe>,
    pub faults: FaultController,
}

pub fn open(name: &str) -> Home {
    let ledger = PathBuf::from(
        std::env::var_os("BERYL_UNCHANGED_FIXTURE_LEDGER")
            .expect("root must supply BERYL_UNCHANGED_FIXTURE_LEDGER"),
    );
    assert!(ledger.is_absolute());
    let path = ledger.parent().unwrap().join(format!(
        "asset-{name}-{}-{}",
        std::process::id(),
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::SeqCst)
    ));
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(ledger)
        .unwrap();
    writeln!(file, "{}", path.display()).unwrap();
    file.sync_all().unwrap();
    std::fs::create_dir(&path).unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let probe = candidate.register_domain::<Probe>().unwrap();
    let store = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .with_domain::<Probe>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    execute(
        &store,
        probe.contribution(store.domain_revision(&probe).unwrap(), PutProbe(1)),
    );
    Home {
        path,
        store,
        state,
        probe,
        faults,
    }
}

pub fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    let outcome = store.execute(command);
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

pub fn image_set(home: &Home) -> SealedAssetReferenceSetProof {
    let sidecar = home
        .store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"unchanged-asset-image",
            SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = home.state.assets();
    let expected = assets.revision(&home.store).unwrap();
    let publication = assets
        .publish_metadata(
            expected,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    publication.add_to(&mut command).unwrap();
    assert!(matches!(
        home.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let begin = BeginAssetReferenceSet::new(AssetReferenceSetStagingAuthority::new(
        AssetReferenceSetId::from_bytes([10; 16]),
        [10; 32],
    ));
    let staging = begin.staging_authority();
    execute(
        &home.store,
        assets.begin_reference_set(assets.revision(&home.store).unwrap(), begin),
    );
    let marker = SyndicDraftMarkerId::from_bytes([11; 16]);
    let label = ImageLabelOrdinal::FIRST;
    let manifest = assets
        .staged_reference_set_manifest(&home.store, staging)
        .unwrap();
    execute(
        &home.store,
        assets.append_reference_page(
            assets.revision(&home.store).unwrap(),
            AppendAssetReferencePage::new(
                manifest.build_proof(),
                Box::from([AssetReferencePageEntry::new(marker, label, asset)]),
            )
            .unwrap(),
        ),
    );
    let manifest = assets
        .staged_reference_set_manifest(&home.store, staging)
        .unwrap();
    let sequential = SequentialMarkerSummaryV1::new(
        advance_sequential_marker_digest(sequential_marker_digest_seed(), marker, label),
        1,
        Some(label),
    )
    .unwrap();
    let ordered = OrderedMarkerAssetSummaryV1::new(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            marker,
            label,
            asset,
        ),
        1,
    );
    let seal = SealAssetReferenceSet::new(manifest.build_proof(), sequential, ordered).unwrap();
    let proof = seal.sealed_proof();
    execute(
        &home.store,
        assets.seal_reference_set(assets.revision(&home.store).unwrap(), seal),
    );
    proof
}
