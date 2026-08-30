use std::{convert::Infallible, num::NonZeroU64};

use beryl_home_store::{
    CommandOutcome, DomainReader, DomainSchemaVersion, FixedDigestHomeProofProtocol, HomeCommand,
    HomeOpenOptions, HomeProofCommand, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion,
    ProofCompositionError, ProofCorrelationBytes, ProofDomain, ProofProtocolIdentity, RecordCodec,
    RecordFamily, RecordVersion, StorageDomain,
};
use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, ImageLabelOrdinal,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    SyndicDraftMarkerId, advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetReferencePageEntry,
    AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState, PublishAssetMetadata,
    SealAssetReferenceSet,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

type Protocol = FixedDigestHomeProofProtocol<0x53444d5244595631, 0x5244595041474531>;

struct SourceDomain;
struct SourceCodec;

impl RecordCodec<SourceDomain> for SourceCodec {
    type Key = u8;
    type Value = ();
    type Error = Infallible;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 0;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*key])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        Ok(encoded.first().copied().unwrap_or_default())
    }

    fn encode_value(_value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }

    fn decode_value(_encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        Ok(())
    }
}

impl StorageDomain for SourceDomain {
    const NAME: &'static str = "phase196-readiness-source";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<SourceCodec>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment(
        _reader: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<Self::RuntimeAttachment, Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

enum NoWitness {}

impl ProofDomain for SourceDomain {
    type SourceInput = [u8; 32];
    type WitnessInput = NoWitness;
    type Error = Infallible;

    fn source_protocol(_input: &Self::SourceInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<Protocol>()
    }

    fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes {
        ProofCorrelationBytes::new(*input)
    }

    fn witness_protocol(input: &Self::WitnessInput) -> ProofProtocolIdentity {
        match *input {}
    }

    fn prove_source(
        input: &Self::SourceInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        Ok(ProofCorrelationBytes::new(*input))
    }

    fn prove_witness(
        input: &Self::WitnessInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        match *input {}
    }
}

struct Fixture {
    store: HomeStore,
    state: BerylState,
    source: beryl_home_store::DomainHandle<SourceDomain>,
    asset_id: AssetId,
    first_proof: SealedAssetReferenceSetProof,
    second_proof: SealedAssetReferenceSetProof,
    label: ImageLabelOrdinal,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempdir().unwrap();
        let mut store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let source = store.register_domain::<SourceDomain>().unwrap();
        let state = BerylState::register(&mut store).unwrap();
        let asset_id = publish_metadata(&store, &state, b"phase196-pinned-asset");
        let label = ImageLabelOrdinal::new(7).unwrap();
        let first_proof = seal_one_entry_set(
            &store,
            &state,
            AssetReferenceSetId::from_bytes([1; 16]),
            marker(1),
            label,
            asset_id,
        );
        let second_proof = seal_one_entry_set(
            &store,
            &state,
            AssetReferenceSetId::from_bytes([2; 16]),
            marker(2),
            label,
            asset_id,
        );
        Self {
            store,
            state,
            source,
            asset_id,
            first_proof,
            second_proof,
            label,
            _directory: directory,
        }
    }

    fn associations(&self) -> Vec<(SealedAssetReferenceSetProof, ImageLabelOrdinal, AssetId)> {
        vec![
            (self.first_proof, self.label, self.asset_id),
            (self.second_proof, self.label, self.asset_id),
        ]
    }

    fn compose(
        &self,
        ordinal: u64,
        eof: bool,
        associations: Vec<(SealedAssetReferenceSetProof, ImageLabelOrdinal, AssetId)>,
    ) -> Result<(), ProofCompositionError> {
        let correlation = page_correlation(ordinal, eof, &associations);
        let witness = (self
            .state
            .assets()
            .draft_marker_label_readiness_witness_factory())(
            &self.store,
            ordinal,
            eof,
            associations,
        )
        .unwrap();
        let source = self.source.proof_source::<Protocol>(
            self.store.domain_revision(&self.source).unwrap(),
            correlation,
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
}

#[test]
fn occurrences_preserve_pinned_digest_while_identical_tuple_reads_coalesce() {
    let fixture = Fixture::new();
    let associations = fixture.associations();
    let digest = page_correlation(3, true, &associations);
    assert_eq!(
        digest,
        [
            34, 224, 39, 79, 71, 115, 29, 135, 237, 137, 162, 125, 246, 250, 8, 168, 83, 28, 206,
            222, 185, 165, 52, 179, 70, 106, 161, 250, 212, 57, 173, 150,
        ]
    );
    fixture.compose(3, true, associations).unwrap();

    let duplicates = vec![(fixture.first_proof, fixture.label, fixture.asset_id); 256];
    assert_ne!(digest, page_correlation(3, true, &duplicates));
    fixture
        .state
        .assets()
        .reset_draft_marker_label_readiness_validation_read_sets_for_test();
    fixture.compose(3, true, duplicates).unwrap();
    assert_eq!(
        fixture
            .state
            .assets()
            .draft_marker_label_readiness_validation_read_sets_for_test(),
        1
    );
}

#[test]
fn page_shape_rejects_disagreement_order_bounds_and_empty_nonterminal_input() {
    let fixture = Fixture::new();
    let different_asset = AssetId::sha256_v1([9; 32], NonZeroU64::new(9).unwrap());

    assert_factory_rejection(
        &fixture,
        1,
        true,
        vec![
            (fixture.first_proof, fixture.label, fixture.asset_id),
            (fixture.second_proof, fixture.label, different_asset),
        ],
    );
    assert!(
        (fixture
            .state
            .assets()
            .draft_marker_label_readiness_witness_factory())(
            &fixture.store,
            1,
            true,
            vec![(fixture.first_proof, fixture.label, fixture.asset_id); 256],
        )
        .is_ok()
    );
    assert_factory_rejection(
        &fixture,
        1,
        true,
        vec![
            (fixture.second_proof, fixture.label, fixture.asset_id),
            (fixture.first_proof, fixture.label, fixture.asset_id),
        ],
    );
    assert_factory_rejection(
        &fixture,
        1,
        true,
        vec![
            (
                fixture.first_proof,
                ImageLabelOrdinal::new(8).unwrap(),
                fixture.asset_id,
            ),
            (fixture.second_proof, fixture.label, fixture.asset_id),
        ],
    );
    assert_factory_rejection(
        &fixture,
        1,
        true,
        vec![(fixture.first_proof, fixture.label, fixture.asset_id); 257],
    );
    assert_factory_rejection(&fixture, 1, false, Vec::new());
    assert_factory_rejection(&fixture, 0, true, Vec::new());
}

#[test]
fn stale_revision_and_missing_malformed_or_disagreeing_authority_reject_composition() {
    let fixture = Fixture::new();
    let ordinal = 4;
    let associations = vec![(fixture.first_proof, fixture.label, fixture.asset_id)];
    let correlation = page_correlation(ordinal, true, &associations);
    let stale_witness = (fixture
        .state
        .assets()
        .draft_marker_label_readiness_witness_factory())(
        &fixture.store,
        ordinal,
        true,
        associations,
    )
    .unwrap();
    publish_metadata(&fixture.store, &fixture.state, b"phase196-revision-advance");
    let source = fixture.source.proof_source::<Protocol>(
        fixture.store.domain_revision(&fixture.source).unwrap(),
        correlation,
    );
    let mut command = HomeProofCommand::new(
        fixture.store.health().generation().unwrap(),
        fixture.store.home_revision().unwrap(),
        source,
    )
    .unwrap();
    command.add_witness(stale_witness).unwrap();
    let (command, _) = command.seal().unwrap();
    assert!(matches!(
        fixture.store.compose_proof(command),
        Err(ProofCompositionError::Conflict { .. })
    ));

    let missing = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([99; 16]),
        fixture.first_proof.sequential(),
        fixture.first_proof.ordered_assets(),
        fixture.first_proof.entry_frontier(),
        fixture.first_proof.asset_chain_digest(),
    )
    .unwrap();
    assert_callback_rejection(&fixture, vec![(missing, fixture.label, fixture.asset_id)]);

    let malformed = SealedAssetReferenceSetProof::new(
        fixture.first_proof.set_id(),
        fixture.first_proof.sequential(),
        fixture.first_proof.ordered_assets(),
        fixture.first_proof.entry_frontier(),
        AssetReferenceSetDigest::from_bytes([44; 32]),
    )
    .unwrap();
    assert_callback_rejection(&fixture, vec![(malformed, fixture.label, fixture.asset_id)]);

    let different_asset = AssetId::sha256_v1([7; 32], NonZeroU64::new(17).unwrap());
    assert_callback_rejection(
        &fixture,
        vec![(fixture.first_proof, fixture.label, different_asset)],
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn missing_asset_metadata_rejects_the_witness_after_valid_reference_sealing() {
    let fixture = Fixture::new();
    execute(
        &fixture.store,
        fixture.state.assets().remove_metadata_for_test(
            fixture.state.assets().revision(&fixture.store).unwrap(),
            fixture.asset_id,
        ),
    );
    assert_callback_rejection(
        &fixture,
        vec![(fixture.first_proof, fixture.label, fixture.asset_id)],
    );
}

fn assert_callback_rejection(
    fixture: &Fixture,
    associations: Vec<(SealedAssetReferenceSetProof, ImageLabelOrdinal, AssetId)>,
) {
    assert!(matches!(
        fixture.compose(8, true, associations),
        Err(ProofCompositionError::Callback {
            domain: "beryl-assets",
            ..
        })
    ));
}

fn assert_factory_rejection(
    fixture: &Fixture,
    ordinal: u64,
    eof: bool,
    associations: Vec<(SealedAssetReferenceSetProof, ImageLabelOrdinal, AssetId)>,
) {
    assert!(
        (fixture
            .state
            .assets()
            .draft_marker_label_readiness_witness_factory())(
            &fixture.store,
            ordinal,
            eof,
            associations,
        )
        .is_err()
    );
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
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

fn publish_metadata(store: &HomeStore, state: &BerylState, bytes: &[u8]) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            beryl_home_store::SidecarNamespace::new("images").unwrap(),
            bytes,
            beryl_home_store::SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let expected = state.assets().revision(store).unwrap();
    let contribution = state
        .assets()
        .publish_metadata(
            expected,
            sidecar,
            PublishAssetMetadata::new(
                asset_id,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected.checked_next().unwrap(),
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
    asset_id
}

fn seal_one_entry_set(
    store: &HomeStore,
    state: &BerylState,
    set_id: AssetReferenceSetId,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> SealedAssetReferenceSetProof {
    let staging = AssetReferenceSetStagingAuthority::new(set_id, [set_id.as_bytes()[0]; 32]);
    let begin = BeginAssetReferenceSet::new(staging);
    execute(
        store,
        state
            .assets()
            .begin_reference_set(state.assets().revision(store).unwrap(), begin),
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
                Box::from([AssetReferencePageEntry::new(marker_id, label, asset_id)]),
            )
            .unwrap(),
        ),
    );
    let manifest = state
        .assets()
        .staged_reference_set_manifest(store, staging)
        .unwrap();
    let seal = SealAssetReferenceSet::new(
        manifest.build_proof(),
        marker_summary([(marker_id, label)]),
        ordered_summary([(marker_id, label, asset_id)]),
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

fn marker(index: u64) -> SyndicDraftMarkerId {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&index.to_be_bytes());
    SyndicDraftMarkerId::from_bytes(bytes)
}

fn marker_summary(
    markers: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal)>,
) -> SequentialMarkerSummaryV1 {
    let mut digest = sequential_marker_digest_seed();
    let mut count = 0;
    let mut maximum = None;
    for (marker, label) in markers {
        digest = advance_sequential_marker_digest(digest, marker, label);
        count += 1;
        maximum = Some(maximum.map_or(label, |prior: ImageLabelOrdinal| prior.max(label)));
    }
    SequentialMarkerSummaryV1::new(digest, count, maximum).unwrap()
}

fn ordered_summary(
    entries: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal, AssetId)>,
) -> OrderedMarkerAssetSummaryV1 {
    let mut digest = ordered_marker_asset_digest_seed();
    let mut count = 0;
    for (marker, label, asset_id) in entries {
        digest = advance_ordered_marker_asset_digest(digest, marker, label, asset_id);
        count += 1;
    }
    OrderedMarkerAssetSummaryV1::new(digest, count)
}

fn page_correlation(
    ordinal: u64,
    eof: bool,
    associations: &[(SealedAssetReferenceSetProof, ImageLabelOrdinal, AssetId)],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"syndic/draft-marker-label-readiness-page/v1");
    hasher.update(ordinal.to_le_bytes());
    hasher.update([u8::from(eof)]);
    hasher.update((associations.len() as u64).to_le_bytes());
    for (proof, label, asset_id) in associations.iter().copied() {
        hasher.update([0x01]);
        hasher.update(proof.set_id().as_bytes());
        hasher.update(proof.sequential().marker_digest());
        hasher.update(proof.sequential().marker_count().to_le_bytes());
        hasher.update(
            proof
                .sequential()
                .maximum_image_label()
                .map_or(0, ImageLabelOrdinal::get)
                .to_le_bytes(),
        );
        hasher.update(proof.ordered_assets().marker_asset_digest());
        hasher.update(proof.ordered_assets().marker_count().to_le_bytes());
        hasher.update(proof.entry_frontier().to_le_bytes());
        hasher.update(proof.asset_chain_digest().as_bytes());
        hasher.update(label.get().to_le_bytes());
        hasher.update([asset_id.version() as u8]);
        hasher.update(asset_id.digest());
        hasher.update(asset_id.length().get().to_le_bytes());
    }
    hasher.finalize().into()
}
