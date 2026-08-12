mod support;

use std::num::NonZeroU64;

use beryl_home_store::{
    CommandError, CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, SidecarByteLimit, SidecarError, SidecarNamespace,
};
use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, ImageLabelOrdinal,
    SealedAssetReferenceSetProof, SealedContentMarkerSummary, SyndicAcceptedInputId,
    SyndicContentDigest, SyndicContentId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId,
    SyndicProjectionId, SyndicRetryRecordId, advance_content_marker_digest,
    content_marker_digest_seed,
};
use beryl_state::{
    ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES, ASSET_REFERENCE_PAGE_MAX_ENTRIES,
    ASSET_REFERENCE_PAGE_MAX_STORED_BYTES, AppendAssetReferencePage, AssetDimensions,
    AssetLabelDisposition, AssetMediaType, AssetMutationError, AssetOwner, AssetOwnerHeadUpdate,
    AssetReadError, AssetReferenceOrdinal, AssetReferencePageEntry, AssetReferencePageError,
    AssetReferenceSetLifecycle, AssetReferenceSetStagingAuthority, BeginAssetReferenceSet,
    BerylState, PublishAssetMetadata, SealAssetReferenceSet, UpdateAssetOwnerHeads,
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
    source: SealedContentMarkerSummary,
) -> AssetReferenceSetStagingAuthority {
    let command = BeginAssetReferenceSet::new(set_id, source);
    let authority = command.staging_authority();
    execute(
        store,
        state
            .assets()
            .begin_reference_set(state.assets().revision(store).unwrap(), command),
    );
    authority
}

fn marker_summary(
    markers: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal)>,
) -> SealedContentMarkerSummary {
    marker_summary_for(
        SyndicContentId::from_bytes([7; 16]),
        SyndicContentDigest::from_bytes([8; 32]),
        markers,
    )
}

fn marker_summary_for(
    content_id: SyndicContentId,
    content_digest: SyndicContentDigest,
    markers: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal)>,
) -> SealedContentMarkerSummary {
    let mut digest = content_marker_digest_seed();
    let mut count = 0_u64;
    let mut maximum = None;
    for (marker, label) in markers {
        digest = advance_content_marker_digest(digest, marker, label);
        count += 1;
        maximum = Some(maximum.map_or(label, |prior: ImageLabelOrdinal| prior.max(label)));
    }
    SealedContentMarkerSummary::new(content_id, content_digest, digest, count, maximum).unwrap()
}

fn publish_metadata(store: &HomeStore, state: &BerylState) -> (AssetId, std::path::PathBuf) {
    publish_metadata_bytes(
        store,
        state,
        b"asset-schema-v2-sidecar",
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
    let staging = begin_reference_set(store, state, set_id, source);
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
    let proof = build.sealed_proof().unwrap();
    execute(
        store,
        state.assets().seal_reference_set(
            state.assets().revision(store).unwrap(),
            SealAssetReferenceSet::new(build, source),
        ),
    );
    let sealed = state
        .assets()
        .sealed_reference_set_manifest(store, proof)
        .unwrap();
    assert_eq!(sealed.sealed_proof(), Some(proof));
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
    let source = marker_summary_for(
        SyndicContentId::from_bytes([content_id_byte; 16]),
        SyndicContentDigest::from_bytes([content_digest_byte; 32]),
        [(marker_id, label)],
    );
    let staging = begin_reference_set(&store, &state, set_id, source);
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
    let proof = build.sealed_proof().unwrap();
    execute(
        &store,
        state.assets().seal_reference_set(
            state.assets().revision(&store).unwrap(),
            SealAssetReferenceSet::new(build, source),
        ),
    );
    let sealed = state
        .assets()
        .sealed_reference_set_manifest(&store, proof)
        .unwrap();
    assert_eq!(proof.asset_chain_digest(), sealed.asset_chain_digest());
    [seed, proof.asset_chain_digest().as_bytes()]
}

#[path = "assets_v2_cases/digest.rs"]
mod digest_cases;
#[path = "assets_v2_cases/lifecycle.rs"]
mod lifecycle_cases;
#[path = "assets_v2_cases/owner_proof.rs"]
mod owner_proof_cases;
#[path = "assets_v2_cases/staging.rs"]
mod staging_cases;
