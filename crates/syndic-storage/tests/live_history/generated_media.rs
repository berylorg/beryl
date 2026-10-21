use super::*;
use beryl_home_store::{CommandOutcome, SidecarByteLimit, SidecarNamespace};
use beryl_model::{AssetId, GeneratedMediaUnavailableReason as Reason};
use beryl_state::{
    AssetMediaType, BerylState, GeneratedMediaFacts, GeneratedMediaLifecycle, GeneratedMediaTarget,
};
use sha2::{Digest, Sha256};
use std::num::NonZeroU64;
#[path = "generated_media/incarnation.rs"]
mod incarnation;
#[path = "generated_media/projection.rs"]
mod projection;
#[path = "generated_media/projection_retained.rs"]
mod projection_retained;
#[path = "generated_media/recovery.rs"]
mod recovery;
#[path = "generated_media/resolution.rs"]
mod resolution;

struct MediaHome {
    home: TestHome,
    store: HomeStore,
    storage: SyndicStorage,
    state: BerylState,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: CasTurnSource,
}
impl MediaHome {
    fn new(name: &str) -> Self {
        Self::with_source_incarnation(
            name,
            Some(beryl_model::GeneratedSourceIncarnation::from_bytes(
                [0x47; 32],
            )),
        )
    }
    fn with_source_incarnation(
        name: &str,
        incarnation: Option<beryl_model::GeneratedSourceIncarnation>,
    ) -> Self {
        let home = TestHome::new(name);
        if let Some(ledger) = std::env::var_os("BERYL_GENERATED_MEDIA_FIXTURE_LEDGER") {
            use std::io::Write;
            let mut ledger = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(ledger)
                .unwrap();
            ledger
                .write_all(format!("{}\n", home.path().display()).as_bytes())
                .unwrap();
            ledger.sync_all().unwrap();
        }
        let mut candidate = open(home.path());
        let state = BerylState::register(&mut candidate).unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let (thread, turn) = seed_pending_turn(&store, &storage);
        let source = crate::support::exact_cas::establish_turn_with_source_incarnation(
            &store,
            storage.clone(),
            thread,
            turn,
            timestamp(4),
            crate::support::exact_cas::tool_profile(),
            incarnation,
        );
        admit_event(
            &store,
            storage.clone(),
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
        correlate_submitted_user_item(&store, &storage, thread, turn, &source, timestamp(5));
        Self {
            home,
            store,
            storage,
            state,
            thread,
            turn,
            source,
        }
    }
    fn frame(
        &self,
        byte: u8,
        completed: bool,
        status: ProviderImageGenerationStatusV1,
        prompt: Option<ProviderTextV1>,
        path: Option<ProviderTextV1>,
        at: u64,
    ) -> SealedProviderFrameReference {
        let item = SyndicItemId::from_bytes([byte; 16]);
        if completed
            && self
                .storage
                .canonical_item(&self.store, item, limit())
                .unwrap()
                .is_none()
        {
            self.frame(
                byte,
                false,
                ProviderImageGenerationStatusV1::InProgress,
                None,
                None,
                at,
            );
        }
        let ordinal = self
            .storage
            .canonical_item(&self.store, item, limit())
            .unwrap()
            .and_then(|item| {
                item.provider()
                    .map(|frame| frame.stream_state().next_ordinal())
            })
            .unwrap_or(ProviderFrameOrdinalV1::FIRST);
        let value = ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
            status,
            revised_prompt: prompt,
            saved_path: path,
        });
        let observed_at = ProviderLifecycleTimestampMsV1::new(at);
        let observation = if completed {
            ProviderItemObservationV1::Completed {
                observed_at,
                item: value,
            }
        } else {
            ProviderItemObservationV1::Started {
                observed_at,
                item: value,
            }
        };
        admit_item_frame(
            &self.store,
            self.storage.clone(),
            self.thread,
            self.turn,
            item,
            &self.source,
            ProviderItemFrameV1::new(
                ordinal,
                CasItemId::new(format!("generated-{byte}")).unwrap(),
                observation,
            ),
            timestamp(at),
        )
    }
    fn resource(&self, byte: u8) -> beryl_model::SyndicResourceId {
        let item = self
            .storage
            .canonical_item(&self.store, SyndicItemId::from_bytes([byte; 16]), limit())
            .unwrap()
            .unwrap();
        let CanonicalItemPresentation::GeneratedMedia { resource_id } = item.presentation() else {
            panic!("generated presentation");
        };
        *resource_id
    }
    fn proof(&self, byte: u8) -> GeneratedMediaSourceProof {
        self.storage
            .generated_media_source(&self.store, self.resource(byte))
            .unwrap()
            .unwrap()
    }
    fn target(proof: &GeneratedMediaSourceProof) -> GeneratedMediaTarget {
        GeneratedMediaTarget::new(
            proof.resource_id(),
            proof.thread_id(),
            proof.turn_id(),
            proof.item_id(),
            proof.source_fingerprint(),
        )
    }
    fn prepare(&self, byte: u8) -> AssetId {
        let proof = self.proof(byte);
        let sidecar = self
            .store
            .admit_sidecar(
                SidecarNamespace::new("images").unwrap(),
                b"durably admitted generated original",
                SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
            )
            .unwrap();
        let asset = AssetId::sha256_v1(
            sidecar.address().digest().as_bytes(),
            NonZeroU64::new(sidecar.address().length()).unwrap(),
        );
        let facts =
            GeneratedMediaFacts::new(asset, AssetMediaType::new("image/png").unwrap(), None)
                .unwrap();
        let prepared = self
            .state
            .assets()
            .prepare_generated_media(
                &self.store,
                self.state.assets().revision(&self.store).unwrap(),
                sidecar,
                Self::target(&proof),
                facts,
            )
            .unwrap();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add_validation(
                self.storage
                    .generated_media_preparation(proof, asset, "image/png", None)
                    .unwrap(),
            )
            .unwrap();
        prepared.add_to(&mut command).unwrap();
        assert_committed(self.store.execute(command));
        asset
    }
    fn unavailable(
        &self,
        proof: GeneratedMediaSourceProof,
        reason: Reason,
        at: u64,
    ) -> CommandOutcome {
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add_validation(self.state.assets().validate_generated_media_absence(
                self.state.assets().revision(&self.store).unwrap(),
                Self::target(&proof),
                reason,
            ))
            .unwrap();
        command
            .add(
                self.storage
                    .unavailable_generated_media(proof, reason, timestamp(at))
                    .unwrap(),
            )
            .unwrap();
        self.store.execute(command)
    }
    fn publication(&self, byte: u8, asset: AssetId, at: u64) -> HomeCommand {
        let proof = self.proof(byte);
        let admission = self
            .state
            .assets()
            .generated_media_admission(&self.store, Self::target(&proof))
            .unwrap()
            .unwrap();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add(
                self.state
                    .assets()
                    .publish_generated_media(
                        &self.store,
                        self.state.assets().revision(&self.store).unwrap(),
                        &admission,
                    )
                    .unwrap(),
            )
            .unwrap();
        command
            .add(
                self.storage
                    .publish_generated_media(proof, asset, "image/png", None, timestamp(at))
                    .unwrap(),
            )
            .unwrap();
        command
    }
}

#[test]
fn generated_resource_birth_is_discoverable_before_completion() {
    let f = MediaHome::new("generated-awaiting");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    let awaiting = f.proof(20);
    assert!(matches!(
        awaiting.source_state(),
        GeneratedMediaSourceState::AwaitingCompletion
    ));
    assert_eq!(awaiting.saved_path(), None);
    assert_eq!(
        awaiting.execution_snapshot().id(),
        awaiting.active_cas_turn().snapshot_id()
    );
    let sweep = f.storage.begin_generated_media_sweep(&f.store).unwrap();
    assert_eq!(sweep.high_water(), 1);
    let page = f
        .storage
        .pending_generated_media_page(&f.store, &sweep, 256, 65_536)
        .unwrap();
    assert_eq!(page.entries()[0].resource_id(), awaiting.resource_id());
    assert!(page.continuation().is_none());
    let asset = AssetId::sha256_v1([3; 32], NonZeroU64::MIN);
    assert!(
        f.storage
            .generated_media_preparation(awaiting.clone(), asset, "image/png", None)
            .is_err()
    );
    assert!(
        f.storage
            .publish_generated_media(awaiting.clone(), asset, "image/png", None, timestamp(7))
            .is_err()
    );
    assert!(
        f.storage
            .unavailable_generated_media(awaiting, Reason::Missing, timestamp(7))
            .is_err()
    );
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn completed_source_extracts_only_saved_path_and_changes_fingerprint_once() {
    let f = MediaHome::new("generated-path");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    let awaiting = f.proof(20).source_fingerprint();
    let path = "C:\\generated\\saved.png";
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        Some(ProviderTextV1::inline("prompt".repeat(30_000))),
        Some(ProviderTextV1::inline(path)),
        7,
    );
    let completed = f.proof(20);
    assert_eq!(completed.saved_path(), Some(Ok(path)));
    assert_ne!(awaiting, completed.source_fingerprint());
    assert_eq!(
        completed.source_fingerprint(),
        f.proof(20).source_fingerprint()
    );
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn completed_source_follows_typed_reused_saved_path() {
    let f = MediaHome::new("generated-reused-path");
    let path = "C:\\generated\\reused.png";
    let first = f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        Some(ProviderTextV1::inline("unretained prompt")),
        Some(ProviderTextV1::inline(path)),
        6,
    );
    let bytes = read_content_bytes(&f.store, &f.storage, first.content().id());
    let start = bytes
        .windows(path.len())
        .position(|bytes| bytes == path.as_bytes())
        .unwrap() as u64;
    let reference = ProviderTextReferenceV1::new(
        start,
        start + path.len() as u64,
        Sha256::digest(path.as_bytes()).into(),
    )
    .unwrap();
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::reused(reference)),
        7,
    );
    assert_eq!(f.proof(20).saved_path(), Some(Ok(path)));
}

#[test]
fn completed_source_classifies_failed_missing_empty_and_oversized_paths() {
    for (index, status, path, reason) in [
        (
            20,
            ProviderImageGenerationStatusV1::Failed,
            Some("ignored".into()),
            Reason::ProducerFailed,
        ),
        (
            21,
            ProviderImageGenerationStatusV1::Completed,
            None,
            Reason::Missing,
        ),
        (
            22,
            ProviderImageGenerationStatusV1::Completed,
            Some(String::new()),
            Reason::Missing,
        ),
        (
            23,
            ProviderImageGenerationStatusV1::Completed,
            Some("x".repeat(4097)),
            Reason::MalformedOutput,
        ),
    ] {
        let f = MediaHome::new("generated-output-classification");
        f.frame(
            index,
            true,
            status,
            None,
            path.map(ProviderTextV1::inline),
            6,
        );
        assert_eq!(f.proof(index).saved_path(), Some(Err(reason)));
        assert_committed(f.unavailable(f.proof(index), reason, 7));
        assert!(
            matches!(f.proof(index).resource().backing(), ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Unavailable(actual)) if actual == reason)
        );
    }
}

#[test]
fn pending_sweep_retains_finite_high_water_across_new_entries_and_unrelated_revisions() {
    let f = MediaHome::new("generated-finite-sweep");
    for byte in 20..23 {
        f.frame(
            byte,
            false,
            ProviderImageGenerationStatusV1::InProgress,
            None,
            None,
            byte as u64,
        );
    }
    let sweep = f.storage.begin_generated_media_sweep(&f.store).unwrap();
    let first = f
        .storage
        .pending_generated_media_page(&f.store, &sweep, 1, 65_536)
        .unwrap();
    assert_eq!(first.entries()[0].sequence(), 1);
    f.frame(
        23,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        24,
    );
    let second = f
        .storage
        .pending_generated_media_page(&f.store, first.continuation().unwrap(), 1, 65_536)
        .unwrap();
    assert_eq!(second.entries()[0].sequence(), 2);
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Failed,
        None,
        None,
        25,
    );
    let third = f
        .storage
        .pending_generated_media_page(&f.store, second.continuation().unwrap(), 256, 65_536)
        .unwrap();
    assert_eq!(third.entries().len(), 1);
    assert_eq!(third.entries()[0].sequence(), 3);
    assert!(third.continuation().is_none());
    let next = f.storage.begin_generated_media_sweep(&f.store).unwrap();
    assert_eq!(next.high_water(), 4);
    assert!(
        f.storage
            .pending_generated_media_page(&f.store, &next, 257, 65_536)
            .is_err()
    );
    assert!(
        f.storage
            .pending_generated_media_page(&f.store, &next, 1, 65_537)
            .is_err()
    );
}
