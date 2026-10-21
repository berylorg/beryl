#![allow(dead_code)]

use beryl_home_store::{
    AdmittedSidecar, CommandError, CommandOutcome, CommandPairRequirement, CommandParticipantKind,
    CommitReceipt, DomainCallbackError, DomainCallbackSource, DomainHandle, DomainMutation,
    DomainReader, DomainReconciliation, DomainSchemaVersion, DomainValidator, HomeCommand,
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion,
    MutationBuildError, MutationBuilder, PointReadLimit, ReadError, ReconciliationReader,
    ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion, SidecarByteLimit,
    SidecarNamespace, StorageDomain, test_faults::FaultController,
};
use beryl_model::{
    AssetId, GeneratedMediaUnavailableReason, SyndicItemId, SyndicResourceId, SyndicThreadId,
    SyndicTurnId,
};
use beryl_state::{
    AssetDimensions, AssetMediaType, BerylState, GeneratedMediaAdmission, GeneratedMediaFacts,
    GeneratedMediaPreparation, GeneratedMediaTarget, PublishAssetMetadata,
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt,
    io::Write,
    num::NonZeroU64,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
pub const BYTES: &[u8] = b"controlled-generated-sidecar-byte-identity";
pub const PROTOCOL: u64 = 0x67656e6d65646961;

#[derive(Debug)]
pub enum TestError {
    Read(ReadError),
    Build(MutationBuildError),
    Rejected,
}
impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(e) => e.fmt(f),
            Self::Build(e) => e.fmt(f),
            Self::Rejected => f.write_str("controlled peer refused"),
        }
    }
}
impl Error for TestError {}
impl DomainCallbackError for TestError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(e) => Ok(DomainCallbackSource::Read(e)),
            e => Err(e),
        }
    }
}
impl From<ReadError> for TestError {
    fn from(e: ReadError) -> Self {
        Self::Read(e)
    }
}
impl From<MutationBuildError> for TestError {
    fn from(e: MutationBuildError) -> Self {
        Self::Build(e)
    }
}

pub struct Peer;
pub struct PeerCodec;
impl StorageDomain for Peer {
    const NAME: &'static str = "syndic";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<PeerCodec>(
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
    fn validate(_: &DomainReader<'_, Self>) -> Result<(), TestError> {
        Ok(())
    }
    fn reconcile(
        reader: &ReconciliationReader<'_, Self>,
    ) -> Result<DomainReconciliation, TestError> {
        let records = reader.records::<PeerCodec>()?;
        let old = records.iter().all(|r| r.current() == r.old());
        let new = records.iter().all(|r| r.current() == r.new());
        Ok(match (records.is_empty(), old, new) {
            (false, true, false) => DomainReconciliation::ExactOld,
            (false, false, true) => DomainReconciliation::ExactNew,
            (false, true, true) => DomainReconciliation::ExactUnchanged,
            _ => DomainReconciliation::Collision,
        })
    }
}
impl RecordCodec<Peer> for PeerCodec {
    type Key = u8;
    type Value = u8;
    type Error = std::io::Error;
    const FAMILY: &'static str = "controlled-generated-resource";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 1;
    fn encode_key(k: &u8) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*k])
    }
    fn decode_key(b: &[u8]) -> Result<u8, Self::Error> {
        one_byte(b)
    }
    fn encode_value(v: &u8) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*v])
    }
    fn decode_value(b: &[u8]) -> Result<u8, Self::Error> {
        one_byte(b)
    }
}
fn one_byte(bytes: &[u8]) -> Result<u8, std::io::Error> {
    match bytes {
        [byte] => Ok(*byte),
        _ => Err(std::io::Error::other("expected exact controlled byte")),
    }
}

pub struct PeerGuard {
    pub pair: Option<CommandPairRequirement>,
    pub reject: bool,
}
impl DomainValidator<Peer> for PeerGuard {
    type Error = TestError;
    fn command_pair(&self) -> Option<CommandPairRequirement> {
        self.pair
    }
    fn validate(&self, reader: &DomainReader<'_, Peer>) -> Result<(), TestError> {
        if self.reject || reader.point::<PeerCodec>(&1, PointReadLimit::new(5).unwrap())? != Some(1)
        {
            return Err(TestError::Rejected);
        }
        Ok(())
    }
}
pub struct PeerWrite {
    pub pair: Option<CommandPairRequirement>,
    pub value: u8,
}
impl DomainMutation<Peer> for PeerWrite {
    type Error = TestError;
    type Prepared = Self;
    fn command_pair(&self) -> Option<CommandPairRequirement> {
        self.pair
    }
    fn prepare(self, _: &DomainReader<'_, Peer>) -> Result<Self, TestError> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        r: &mut ReconciliationReservation<'_, Peer>,
    ) -> Result<(), TestError> {
        r.reserve_records::<PeerCodec>(1)?;
        Ok(())
    }
    fn contribute(p: Self, w: &mut MutationBuilder<'_, Peer>) -> Result<(), TestError> {
        w.put::<PeerCodec>(&1, &p.value)?;
        Ok(())
    }
}

pub struct Home {
    pub path: PathBuf,
    pub store: HomeStore,
    pub state: BerylState,
    pub peer: DomainHandle<Peer>,
    pub faults: FaultController,
}
pub fn open(name: &str) -> Home {
    let ledger = PathBuf::from(
        std::env::var_os("BERYL_GENERATED_MEDIA_FIXTURE_LEDGER")
            .expect("root must supply BERYL_GENERATED_MEDIA_FIXTURE_LEDGER"),
    );
    assert!(ledger.is_absolute());
    let path = ledger.parent().unwrap().join(format!(
        "state-generated-{name}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::SeqCst)
    ));
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(ledger)
        .unwrap();
    writeln!(file, "{}", path.display()).unwrap();
    file.sync_all().unwrap();
    std::fs::create_dir(&path).unwrap();
    open_at(&path)
}
pub fn open_at(path: &Path) -> Home {
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let peer = candidate.register_domain::<Peer>().unwrap();
    let store = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .with_domain::<Peer>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let home = Home {
        path: path.to_path_buf(),
        store,
        state,
        peer,
        faults,
    };
    if home.peer_value().is_none() {
        let mut command = HomeCommand::new(home.store.home_revision().unwrap());
        command
            .add(home.peer.contribution(
                home.peer_revision(),
                PeerWrite {
                    pair: None,
                    value: 1,
                },
            ))
            .unwrap();
        committed(home.store.execute(command));
    }
    home
}
impl Home {
    pub fn peer_revision(&self) -> beryl_model::DomainRevision {
        self.store.domain_revision(&self.peer).unwrap()
    }
    pub fn peer_value(&self) -> Option<u8> {
        self.store
            .read_point::<Peer, PeerCodec>(&self.peer, &1, PointReadLimit::new(5).unwrap())
            .unwrap()
    }
    pub fn revision(&self) -> beryl_model::DomainRevision {
        self.state.assets().revision(&self.store).unwrap()
    }
    pub fn preparation(&self, target: GeneratedMediaTarget) -> GeneratedMediaPreparation {
        let sidecar = sidecar(&self.store, BYTES, "images");
        let facts = facts(&sidecar, Some(dimensions(20, 30)));
        self.state
            .assets()
            .prepare_generated_media(&self.store, self.revision(), sidecar, target, facts)
            .unwrap()
    }
    pub fn preparation_command(&self, target: GeneratedMediaTarget) -> HomeCommand {
        let preparation = self.preparation(target);
        let digest = asset_digest(1, target, preparation.facts());
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        preparation.add_to(&mut command).unwrap();
        command
            .add_validation(self.peer.validation(
                self.peer_revision(),
                PeerGuard {
                    pair: Some(pair::<1>(CommandParticipantKind::Mutation, digest)),
                    reject: false,
                },
            ))
            .unwrap();
        command
    }
    pub fn prepared(&self, target: GeneratedMediaTarget) -> GeneratedMediaAdmission {
        committed(self.store.execute(self.preparation_command(target)));
        self.admission(target)
    }
    pub fn admission(&self, target: GeneratedMediaTarget) -> GeneratedMediaAdmission {
        self.state
            .assets()
            .generated_media_admission(&self.store, target)
            .unwrap()
            .unwrap()
    }
    pub fn publication_command(&self, admission: &GeneratedMediaAdmission) -> HomeCommand {
        let contribution = self
            .state
            .assets()
            .publish_generated_media(&self.store, self.revision(), admission)
            .unwrap();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command.add(contribution).unwrap();
        command
            .add(self.peer.contribution(
                self.peer_revision(),
                PeerWrite {
                    pair: Some(pair::<2>(
                        CommandParticipantKind::Mutation,
                        asset_digest(2, admission.target(), admission.facts()),
                    )),
                    value: 2,
                },
            ))
            .unwrap();
        command
    }
    pub fn unavailable_command(
        &self,
        target: GeneratedMediaTarget,
        reason: GeneratedMediaUnavailableReason,
    ) -> HomeCommand {
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add_validation(self.state.assets().validate_generated_media_absence(
                self.revision(),
                target,
                reason,
            ))
            .unwrap();
        command
            .add(self.peer.contribution(
                self.peer_revision(),
                PeerWrite {
                    pair: Some(pair::<3>(
                        CommandParticipantKind::Validation,
                        unavailable_digest(target, reason),
                    )),
                    value: 3,
                },
            ))
            .unwrap();
        command
    }
}

pub fn target() -> GeneratedMediaTarget {
    GeneratedMediaTarget::new(
        SyndicResourceId::from_bytes([1; 16]),
        SyndicThreadId::from_bytes([2; 16]),
        SyndicTurnId::from_bytes([3; 16]),
        SyndicItemId::from_bytes([4; 16]),
        [5; 32],
    )
}
pub fn dimensions(w: u64, h: u64) -> AssetDimensions {
    AssetDimensions::new(NonZeroU64::new(w).unwrap(), NonZeroU64::new(h).unwrap())
}
pub fn sidecar(store: &HomeStore, bytes: &[u8], namespace: &str) -> AdmittedSidecar {
    store
        .admit_sidecar(
            SidecarNamespace::new(namespace).unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap()
}
pub fn facts(
    sidecar: &AdmittedSidecar,
    dimensions: Option<AssetDimensions>,
) -> GeneratedMediaFacts {
    GeneratedMediaFacts::new(
        AssetId::sha256_v1(
            sidecar.address().digest().as_bytes(),
            NonZeroU64::new(sidecar.address().length()).unwrap(),
        ),
        AssetMediaType::new("image/png").unwrap(),
        dimensions,
    )
    .unwrap()
}
pub fn seed_metadata(home: &Home, media: &str, dimensions: Option<AssetDimensions>) -> AssetId {
    let sidecar = sidecar(&home.store, BYTES, "images");
    let asset = facts(&sidecar, dimensions).asset();
    let publish = home
        .state
        .assets()
        .publish_metadata(
            home.revision(),
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new(media).unwrap(),
                dimensions,
                home.revision().checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    publish.add_to(&mut command).unwrap();
    committed(home.store.execute(command));
    asset
}
pub fn committed(outcome: CommandOutcome) -> CommitReceipt {
    match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
            ..
        } => receipt,
        outcome => panic!("expected committed: {outcome:?}"),
    }
}
pub fn rejected(outcome: CommandOutcome) -> CommandError {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected not committed: {outcome:?}"),
    }
}
pub fn pair<const OP: u64>(
    kind: CommandParticipantKind,
    digest: [u8; 32],
) -> CommandPairRequirement {
    CommandPairRequirement::new::<PROTOCOL, OP>("beryl-assets", kind, digest)
}
pub fn prefix(operation: u64, target: GeneratedMediaTarget) -> Sha256 {
    let mut hash = Sha256::new();
    hash.update(b"beryl/generated-media-command/v1\0");
    hash.update(operation.to_le_bytes());
    hash.update(target.resource().as_bytes());
    hash.update(target.thread().as_bytes());
    hash.update(target.turn().as_bytes());
    hash.update(target.item().as_bytes());
    hash.update(target.fingerprint());
    hash
}
pub fn asset_digest(
    operation: u64,
    target: GeneratedMediaTarget,
    facts: &GeneratedMediaFacts,
) -> [u8; 32] {
    let mut hash = prefix(operation, target);
    hash.update(facts.asset().digest());
    hash.update(facts.asset().length().get().to_le_bytes());
    hash.update([u8::try_from(facts.media_type().as_str().len()).unwrap()]);
    hash.update(facts.media_type().as_str().as_bytes());
    match facts.dimensions() {
        None => hash.update([0]),
        Some(dimensions) => {
            hash.update([1]);
            hash.update(
                u32::try_from(dimensions.width().get())
                    .unwrap()
                    .to_le_bytes(),
            );
            hash.update(
                u32::try_from(dimensions.height().get())
                    .unwrap()
                    .to_le_bytes(),
            );
        }
    }
    hash.finalize().into()
}
pub fn unavailable_digest(
    target: GeneratedMediaTarget,
    reason: GeneratedMediaUnavailableReason,
) -> [u8; 32] {
    let mut hash = prefix(3, target);
    hash.update([reason.code()]);
    hash.finalize().into()
}
