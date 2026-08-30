mod support;

use std::num::NonZeroU64;

use beryl_home_store::{
    CommandError, CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, SidecarByteLimit, SidecarError, SidecarNamespace,
};
use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, ImageLabelOrdinal,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    SyndicAcceptedInputId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicProjectionId,
    SyndicRetryRecordId, advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};
use beryl_state::{
    ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES, ASSET_REFERENCE_PAGE_MAX_ENTRIES,
    ASSET_REFERENCE_PAGE_MAX_STORED_BYTES, AppendAssetReferencePage, AssetDimensions,
    AssetLabelDisposition, AssetMediaType, AssetMutationError, AssetOwner, AssetOwnerHeadUpdate,
    AssetReadError, AssetReferenceOrdinal, AssetReferencePageEntry, AssetReferencePageError,
    AssetReferenceSetCompletion, AssetReferenceSetLifecycle, AssetReferenceSetStagingAuthority,
    BeginAssetReferenceSet, BerylState, PublishAssetMetadata, SealAssetReferenceSet,
    UpdateAssetOwnerHeads,
};
use tempfile::tempdir;

fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> beryl_home_store::CommitReceipt {
    match execute_outcome(store, contribution) {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
            local_finalization: _,
        } => receipt,
        outcome => panic!("expected committed asset command, got {outcome:?}"),
    }
}

fn execute_outcome(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn marker(index: u64) -> SyndicDraftMarkerId {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&index.to_be_bytes());
    SyndicDraftMarkerId::from_bytes(bytes)
}

fn begin_reference_set(
    store: &HomeStore,
    state: &BerylState,
    set_id: AssetReferenceSetId,
) -> AssetReferenceSetStagingAuthority {
    let command = BeginAssetReferenceSet::new(AssetReferenceSetStagingAuthority::new(
        set_id,
        [set_id.as_bytes()[0]; 32],
    ));
    let authority = command.staging_authority();
    execute(
        store,
        state
            .assets()
            .begin_reference_set(state.assets().revision(store).unwrap(), command),
    );
    authority
}

fn ordered_marker_asset_summary(
    entries: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal, AssetId)>,
) -> OrderedMarkerAssetSummaryV1 {
    let mut digest = ordered_marker_asset_digest_seed();
    let mut count = 0_u64;
    for (marker, label, asset_id) in entries {
        digest = advance_ordered_marker_asset_digest(digest, marker, label, asset_id);
        count += 1;
    }
    OrderedMarkerAssetSummaryV1::new(digest, count)
}

fn marker_summary(
    markers: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal)>,
) -> SequentialMarkerSummaryV1 {
    marker_summary_for(markers)
}

fn marker_summary_for(
    markers: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal)>,
) -> SequentialMarkerSummaryV1 {
    let mut digest = sequential_marker_digest_seed();
    let mut count = 0_u64;
    let mut maximum = None;
    for (marker, label) in markers {
        digest = advance_sequential_marker_digest(digest, marker, label);
        count += 1;
        maximum = Some(maximum.map_or(label, |prior: ImageLabelOrdinal| prior.max(label)));
    }
    SequentialMarkerSummaryV1::new(digest, count, maximum).unwrap()
}

fn publish_metadata(store: &HomeStore, state: &BerylState) -> (AssetId, std::path::PathBuf) {
    publish_metadata_bytes(
        store,
        state,
        b"asset-schema-v3-sidecar",
        AssetMediaType::new("image/png").unwrap(),
        None,
    )
}

fn publish_metadata_bytes(
    store: &HomeStore,
    state: &BerylState,
    bytes: &[u8],
    media_type: AssetMediaType,
    dimensions: Option<AssetDimensions>,
) -> (AssetId, std::path::PathBuf) {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let path = sidecar.path().to_path_buf();
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
                media_type,
                dimensions,
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
        outcome => panic!("expected committed asset metadata publication, got {outcome:?}"),
    }
    (asset_id, path)
}

fn asset_mutation_error(error: &CommandError) -> &AssetMutationError {
    support::contributor_source::<AssetMutationError>(error)
        .expect("asset command must expose its typed mutation rejection")
}

fn seal_one_entry_set(
    store: &HomeStore,
    state: &BerylState,
    set_id: AssetReferenceSetId,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> SealedAssetReferenceSetProof {
    let source = marker_summary([(marker_id, label)]);
    let staging = begin_reference_set(store, state, set_id);
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
    let build = manifest.build_proof();
    let seal = SealAssetReferenceSet::new(
        build,
        source,
        ordered_marker_asset_summary([(marker_id, label, asset_id)]),
    )
    .unwrap();
    let proof = seal.sealed_proof();
    execute(
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

fn digest_vector(
    set_byte: u8,
    content_id_byte: u8,
    content_digest_byte: u8,
    marker_index: u64,
    label: ImageLabelOrdinal,
    asset_bytes: &[u8],
) -> [[u8; 32]; 2] {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, _) = publish_metadata_bytes(
        &store,
        &state,
        asset_bytes,
        AssetMediaType::new("image/png").unwrap(),
        None,
    );
    let set_id = AssetReferenceSetId::from_bytes([set_byte; 16]);
    let marker_id = marker(marker_index);
    let _ = (content_id_byte, content_digest_byte);
    let source = marker_summary_for([(marker_id, label)]);
    let staging = begin_reference_set(&store, &state, set_id);
    let building = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    let seed = building.asset_chain_digest().as_bytes();
    execute(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                building.build_proof(),
                Box::from([AssetReferencePageEntry::new(marker_id, label, asset_id)]),
            )
            .unwrap(),
        ),
    );
    let building = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    let build = building.build_proof();
    let seal = SealAssetReferenceSet::new(
        build,
        source,
        ordered_marker_asset_summary([(marker_id, label, asset_id)]),
    )
    .unwrap();
    let proof = seal.sealed_proof();
    execute(
        &store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(&store).unwrap(), seal),
    );
    let sealed = state
        .assets()
        .sealed_reference_set_manifest(&store, proof)
        .unwrap();
    assert_eq!(proof.asset_chain_digest(), sealed.asset_chain_digest());
    [seed, proof.asset_chain_digest().as_bytes()]
}

#[path = "assets_v3_cases/completion.rs"]
mod completion_cases;
#[path = "assets_v3_cases/digest.rs"]
mod digest_cases;
#[path = "assets_v3_cases/lifecycle.rs"]
mod lifecycle_cases;
#[path = "assets_v3_cases/owner_proof.rs"]
mod owner_proof_cases;
#[path = "assets_v3_cases/staging.rs"]
mod staging_cases;
