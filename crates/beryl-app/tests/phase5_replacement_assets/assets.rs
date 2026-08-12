use std::num::NonZeroU64;

use beryl_home_store::{HomeCommand, HomeStore, SidecarByteLimit, SidecarNamespace};
use beryl_model::{
    AssetId, AssetReferenceSetId, ContentRevision, SealedAssetReferenceSetProof,
    SyndicDraftMarkerId,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadUpdate,
    AssetReferencePageEntry, BeginAssetReferenceSet, BerylState, PublishAssetMetadata,
    SealAssetReferenceSet, UpdateAssetOwnerHeads,
};
use syndic_storage::{ComposerAtom, ComposerPayload, ImageLabelOrdinal, PreparedContent};

pub(super) fn historical_content(marker_id: SyndicDraftMarkerId) -> PreparedContent {
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("historical").unwrap(),
        ComposerAtom::image_marker(marker_id, ImageLabelOrdinal::FIRST),
    ])
    .unwrap();
    PreparedContent::composer(&payload).unwrap()
}

pub(super) fn create_historical_asset(
    store: &mut HomeStore,
    state: BerylState,
    owner: Option<AssetOwner>,
    marker_id: SyndicDraftMarkerId,
    set_seed: u8,
    bytes: &[u8],
) -> (AssetId, SealedAssetReferenceSetProof) {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1_024 * 1_024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = state.assets();
    let revision = assets.revision(store).unwrap();
    let metadata = assets
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
    metadata.add_to(&mut command).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        beryl_home_store::CommandOutcome::NotCommitted { evidence } => {
            panic!("expected committed asset setup, got not committed: {evidence:?}")
        }
        outcome @ beryl_home_store::CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => {
            panic!("expected committed asset setup with no later failure: {outcome:?}")
        }
        outcome @ beryl_home_store::CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed asset setup, got indeterminate: {outcome:?}")
        }
    }

    let source = historical_content(marker_id)
        .reference(ContentRevision::new(1).unwrap())
        .sealed_marker_summary()
        .unwrap();
    let begin =
        BeginAssetReferenceSet::new(AssetReferenceSetId::from_bytes([set_seed; 16]), source);
    let staging = begin.staging_authority();
    execute_asset(
        store,
        assets.begin_reference_set(assets.revision(store).unwrap(), begin),
    );
    let build = assets
        .staged_reference_set_manifest(store, staging)
        .unwrap()
        .build_proof();
    execute_asset(
        store,
        assets.append_reference_page(
            assets.revision(store).unwrap(),
            AppendAssetReferencePage::new(
                build,
                Box::from([AssetReferencePageEntry::new(
                    marker_id,
                    ImageLabelOrdinal::FIRST,
                    asset,
                )]),
            )
            .unwrap(),
        ),
    );
    let build = assets
        .staged_reference_set_manifest(store, staging)
        .unwrap()
        .build_proof();
    let proof = build.sealed_proof().unwrap();
    execute_asset(
        store,
        assets.seal_reference_set(
            assets.revision(store).unwrap(),
            SealAssetReferenceSet::new(build, source),
        ),
    );
    if let Some(owner) = owner {
        execute_asset(
            store,
            assets.update_owner_heads(
                assets.revision(store).unwrap(),
                UpdateAssetOwnerHeads::new(
                    vec![AssetOwnerHeadUpdate::replace(owner, None, Some(proof))]
                        .into_boxed_slice(),
                )
                .unwrap(),
            ),
        );
    }
    (asset, proof)
}

fn execute_asset(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed asset mutation, got {outcome:?}"),
    }
}
