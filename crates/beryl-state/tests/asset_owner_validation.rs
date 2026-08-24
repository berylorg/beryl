mod support;

use std::{convert::Infallible, num::NonZeroU64};

use beryl_home_store::{
    CommandError, CommandOutcome, DomainMutation, DomainReader, DomainSchemaVersion, HomeCommand,
    HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuilder,
    ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion, SidecarByteLimit,
    SidecarNamespace, StorageDomain,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, ImageLabelOrdinal, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof, SequentialMarkerSummaryV1, SyndicDraftId, SyndicDraftMarkerId,
    SyndicItemId, advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadAssertion,
    AssetOwnerHeadUpdate, AssetOwnerHeadUpdateError, AssetOwnerHeadValidationError,
    AssetReferencePageEntry, BeginAssetReferenceSet, BerylState, PublishAssetMetadata,
    RecordRevision, SealAssetReferenceSet, UpdateAssetOwnerHeads, ValidateAssetOwnerHeads,
};
use tempfile::tempdir;

struct ProbeDomain;
struct ProbeRecord;

const PROBE_FAMILIES: &[RecordFamily<ProbeDomain>] = &[RecordFamily::new::<ProbeRecord>(
    KeyspaceSchemaVersion::new(1),
)];

impl StorageDomain for ProbeDomain {
    const NAME: &'static str = "asset-owner-validation-probe";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = PROBE_FAMILIES;
    type ValidationError = Infallible;

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl RecordCodec<ProbeDomain> for ProbeRecord {
    type Key = u8;
    type Value = u8;
    type Error = Infallible;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 1;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*key])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        Ok(encoded[0])
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*value])
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        Ok(encoded[0])
    }
}

struct PutProbe {
    key: u8,
    value: u8,
}

impl DomainMutation<ProbeDomain> for PutProbe {
    type Error = Infallible;

    fn validate(&self, _reader: &DomainReader<'_, ProbeDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, ProbeDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ProbeRecord>(1).unwrap();
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, ProbeDomain>,
        mutations: &mut MutationBuilder<'_, ProbeDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<ProbeRecord>(&self.key, &self.value)
            .expect("probe codec is infallible");
        Ok(())
    }
}

fn execute_asset(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed asset-owner validation command, got {outcome:?}"),
    }
}

fn marker() -> SyndicDraftMarkerId {
    SyndicDraftMarkerId::from_bytes([7; 16])
}

fn marker_summary() -> SequentialMarkerSummaryV1 {
    let label = ImageLabelOrdinal::FIRST;
    SequentialMarkerSummaryV1::new(
        advance_sequential_marker_digest(sequential_marker_digest_seed(), marker(), label),
        1,
        Some(label),
    )
    .unwrap()
}

fn ordered_marker_asset_summary(asset_id: AssetId) -> OrderedMarkerAssetSummaryV1 {
    OrderedMarkerAssetSummaryV1::new(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            marker(),
            ImageLabelOrdinal::FIRST,
            asset_id,
        ),
        1,
    )
}

fn publish_asset(store: &HomeStore, state: &BerylState) -> AssetId {
    let bytes = b"asset-owner-validation-sidecar";
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1_024).unwrap()),
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
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed asset metadata command, got {outcome:?}"),
    }
    asset_id
}

fn sealed_set(
    store: &HomeStore,
    state: &BerylState,
    asset_id: AssetId,
) -> SealedAssetReferenceSetProof {
    let set_id = AssetReferenceSetId::from_bytes([10; 16]);
    let source = marker_summary();
    let begin = BeginAssetReferenceSet::new(set_id);
    let staging = begin.staging_authority();
    execute_asset(
        store,
        state
            .assets()
            .begin_reference_set(state.assets().revision(store).unwrap(), begin),
    );
    let manifest = state
        .assets()
        .staged_reference_set_manifest(store, staging)
        .unwrap();
    execute_asset(
        store,
        state.assets().append_reference_page(
            state.assets().revision(store).unwrap(),
            AppendAssetReferencePage::new(
                manifest.build_proof(),
                Box::from([AssetReferencePageEntry::new(
                    marker(),
                    ImageLabelOrdinal::FIRST,
                    asset_id,
                )]),
            )
            .unwrap(),
        ),
    );
    let manifest = state
        .assets()
        .staged_reference_set_manifest(store, staging)
        .unwrap();
    let build = manifest.build_proof();
    let seal =
        SealAssetReferenceSet::new(build, source, ordered_marker_asset_summary(asset_id)).unwrap();
    let proof = seal.sealed_proof();
    execute_asset(
        store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(store).unwrap(), seal),
    );
    let sealed = state
        .assets()
        .sealed_reference_set_manifest(store, proof)
        .unwrap();
    assert_eq!(sealed.sequential(), proof.sequential());
    assert_eq!(sealed.ordered_assets(), proof.ordered_assets());
    proof
}

#[path = "asset_owner_validation_cases/absence.rs"]
mod absence_cases;
#[path = "asset_owner_validation_cases/corruption.rs"]
mod corruption_cases;
#[path = "asset_owner_validation_cases/present.rs"]
mod present_cases;
