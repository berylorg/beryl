use std::{
    io::Write,
    num::NonZeroU64,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeProofCommand,
    HomeSchemaVersion, HomeStore, MutationContribution, ProofCompositionError,
    ProofWitnessContribution, SidecarByteLimit, SidecarNamespace, VerifiedSidecar,
    test_faults::FaultController,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, HistoricalInputMarkerAssociationV1, HistoricalInputMarkerPageV1,
    ImageLabelOrdinal, OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof,
    SequentialMarkerSummaryV1, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId,
    advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadUpdate,
    AssetReferencePageEntry, AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState,
    HistoricalInputMarkerProofProtocolV1, PublishAssetMetadata, SealAssetReferenceSet,
    UpdateAssetOwnerHeads,
};

use super::source::{SourceDomain, SourceInput, correlation};

static FIXTURE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
pub const IMAGE_BYTES: &[u8] = b"local historical image bytes";

pub struct Fixture {
    pub store: HomeStore,
    pub state: BerylState,
    pub source: beryl_home_store::DomainHandle<SourceDomain>,
    pub faults: FaultController,
    pub asset: AssetId,
    pub sidecar_path: PathBuf,
    pub item: SyndicItemId,
    pub proof: SealedAssetReferenceSetProof,
    pub path: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let path = owned_directory();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let source = candidate.register_domain::<SourceDomain>().unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .with_domain::<SourceDomain>()
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let (asset, sidecar_path) = publish_asset(&store, &state, IMAGE_BYTES);
        let label = label(7);
        let proof = seal_set(
            &store,
            &state,
            1,
            &[(marker(1), label, asset), (marker(2), label, asset)],
        );
        let item = SyndicItemId::from_bytes([4; 16]);
        execute(
            &store,
            state.assets().update_owner_heads(
                state.assets().revision(&store).unwrap(),
                UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                    AssetOwner::SubmittedTurnItem(item),
                    None,
                    Some(proof),
                )]))
                .unwrap(),
            ),
        );
        Self {
            store,
            state,
            source,
            faults,
            asset,
            sidecar_path,
            item,
            proof,
            path,
        }
    }

    pub fn page(&self, ordinal: u64) -> HistoricalInputMarkerPageV1 {
        HistoricalInputMarkerPageV1::new(
            SyndicDraftId::from_bytes([9; 16]),
            [10; 16],
            [11; 16],
            ordinal,
            [12; 32],
            b"component-proof-import-header".to_vec(),
            self.item,
            self.proof,
            (ordinal - 1) * 25,
            ordinal,
            HistoricalInputMarkerAssociationV1::new(
                ordinal,
                marker(ordinal as u8),
                label(7),
                self.asset,
            )
            .unwrap(),
        )
        .unwrap()
    }

    pub fn verified(&self) -> Arc<VerifiedSidecar> {
        Arc::new(
            self.state
                .assets()
                .verify_sidecar(&self.store, self.asset)
                .unwrap(),
        )
    }

    pub fn witness(
        &self,
        page: HistoricalInputMarkerPageV1,
        verified: Arc<VerifiedSidecar>,
    ) -> ProofWitnessContribution<HistoricalInputMarkerProofProtocolV1> {
        self.state
            .assets()
            .historical_input_marker_witness_with_verified_sidecar(&self.store, page, verified)
            .unwrap()
    }

    pub fn compose(
        &self,
        page: &HistoricalInputMarkerPageV1,
        witness: ProofWitnessContribution<HistoricalInputMarkerProofProtocolV1>,
    ) -> Result<(), ProofCompositionError> {
        let source = self
            .source
            .proof_source::<HistoricalInputMarkerProofProtocolV1>(
                self.store.domain_revision(&self.source).unwrap(),
                SourceInput {
                    correlation: correlation(page),
                    wrong_protocol: false,
                },
            );
        let mut command = HomeProofCommand::new(
            self.store.health().generation().unwrap(),
            self.store.home_revision().unwrap(),
            source,
        )
        .unwrap();
        command.add_witness(witness).unwrap();
        let (command, consumer) = command.seal().unwrap();
        let receipt = self.store.compose_proof(command)?;
        self.store.consume_proof_receipt(consumer, receipt).unwrap();
        Ok(())
    }

    pub fn assert_unchanged(&self, action: impl FnOnce()) {
        let home_revision = self.store.home_revision().unwrap();
        let asset_revision = self.state.assets().revision(&self.store).unwrap();
        let source_revision = self.store.domain_revision(&self.source).unwrap();
        let owner = AssetOwner::SubmittedTurnItem(self.item);
        let head = self.state.assets().owner_head(&self.store, owner).unwrap();
        let metadata = self
            .state
            .assets()
            .metadata(&self.store, self.asset)
            .unwrap();
        action();
        assert_eq!(self.store.home_revision().unwrap(), home_revision);
        assert_eq!(
            self.state.assets().revision(&self.store).unwrap(),
            asset_revision
        );
        assert_eq!(
            self.store.domain_revision(&self.source).unwrap(),
            source_revision
        );
        assert_eq!(
            self.state.assets().owner_head(&self.store, owner).unwrap(),
            head
        );
        assert_eq!(
            self.state
                .assets()
                .metadata(&self.store, self.asset)
                .unwrap(),
            metadata
        );
    }

    pub fn assert_callback_rejected(&self, page: HistoricalInputMarkerPageV1) {
        let verified = self.verified();
        self.assert_unchanged(|| {
            let witness = self.witness(page.clone(), verified);
            assert!(matches!(self.compose(&page, witness),
                Err(ProofCompositionError::Callback { source, .. })
                    if source.is::<beryl_state::AssetDraftMarkerLabelReadinessError>()));
        });
    }
}

fn owned_directory() -> PathBuf {
    let ledger = PathBuf::from(
        std::env::var_os("BERYL_REPLACEMENT_FIXTURE_LEDGER")
            .expect("root must supply BERYL_REPLACEMENT_FIXTURE_LEDGER"),
    );
    assert!(ledger.is_absolute());
    let run_root = ledger.parent().unwrap().canonicalize().unwrap();
    let path = run_root.join(format!(
        "historical-input-proof-{}-{}",
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

pub fn execute(store: &HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

pub fn publish_asset(store: &HomeStore, state: &BerylState, bytes: &[u8]) -> (AssetId, PathBuf) {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let path = sidecar.path().to_path_buf();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let revision = state.assets().revision(store).unwrap();
    let contribution = state
        .assets()
        .publish_metadata(
            revision,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                None,
                revision.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    (asset, path)
}

pub fn seal_set(
    store: &HomeStore,
    state: &BerylState,
    id: u8,
    entries: &[(SyndicDraftMarkerId, ImageLabelOrdinal, AssetId)],
) -> SealedAssetReferenceSetProof {
    let staging =
        AssetReferenceSetStagingAuthority::new(AssetReferenceSetId::from_bytes([id; 16]), [id; 32]);
    execute(
        store,
        state.assets().begin_reference_set(
            state.assets().revision(store).unwrap(),
            BeginAssetReferenceSet::new(staging),
        ),
    );
    let manifest = state
        .assets()
        .staged_reference_set_manifest(store, staging)
        .unwrap();
    execute(
        store,
        state.assets().append_reference_page(
            state.assets().revision(store).unwrap(),
            AppendAssetReferencePage::new(
                manifest.build_proof(),
                entries
                    .iter()
                    .map(|&(marker, label, asset)| {
                        AssetReferencePageEntry::new(marker, label, asset)
                    })
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            )
            .unwrap(),
        ),
    );
    let manifest = state
        .assets()
        .staged_reference_set_manifest(store, staging)
        .unwrap();
    let mut sequential = sequential_marker_digest_seed();
    let mut ordered = ordered_marker_asset_digest_seed();
    let mut maximum = None;
    for &(marker, label, asset) in entries {
        sequential = advance_sequential_marker_digest(sequential, marker, label);
        ordered = advance_ordered_marker_asset_digest(ordered, marker, label, asset);
        maximum = Some(maximum.map_or(label, |old: ImageLabelOrdinal| old.max(label)));
    }
    let seal = SealAssetReferenceSet::new(
        manifest.build_proof(),
        SequentialMarkerSummaryV1::new(sequential, entries.len() as u64, maximum).unwrap(),
        OrderedMarkerAssetSummaryV1::new(ordered, entries.len() as u64),
    )
    .unwrap();
    let proof = seal.sealed_proof();
    execute(
        store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(store).unwrap(), seal),
    );
    proof
}

pub fn marker(value: u8) -> SyndicDraftMarkerId {
    SyndicDraftMarkerId::from_bytes([value; 16])
}

pub fn label(value: u64) -> ImageLabelOrdinal {
    ImageLabelOrdinal::new(value).unwrap()
}

pub fn with_association(
    page: &HistoricalInputMarkerPageV1,
    association: HistoricalInputMarkerAssociationV1,
) -> HistoricalInputMarkerPageV1 {
    with_owner_and_proof(page, page.item_id(), page.proof(), association)
}

pub fn with_owner_and_proof(
    page: &HistoricalInputMarkerPageV1,
    item: SyndicItemId,
    proof: SealedAssetReferenceSetProof,
    association: HistoricalInputMarkerAssociationV1,
) -> HistoricalInputMarkerPageV1 {
    HistoricalInputMarkerPageV1::new(
        page.draft_id(),
        page.session_id(),
        page.operation_id(),
        page.receipt_ordinal(),
        page.receipt_digest(),
        page.canonical_header().to_vec(),
        item,
        proof,
        page.encoded_start(),
        page.source_piece(),
        association,
    )
    .unwrap()
}
