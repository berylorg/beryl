use std::num::NonZeroU64;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeStore, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::{AssetId, AssetReferenceSetId, SyndicDraftMarkerId};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadUpdate,
    AssetReferencePageEntry, AssetState, BeginAssetReferenceSet, PublishAssetMetadata,
    SealAssetReferenceSet, UpdateAssetOwnerHeads,
};
use syndic_storage::{ContentReference, ImageLabelOrdinal};

use super::execute_one;

#[allow(clippy::too_many_arguments)]
pub(super) fn admit_owner_set(
    store: &HomeStore,
    assets: AssetState,
    content: ContentReference,
    marker: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    owner: AssetOwner,
    set_seed: u8,
    bytes: &[u8],
) -> beryl_model::SealedAssetReferenceSetProof {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1_024 * 1_024).unwrap()),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let revision = assets.revision(store).unwrap();
    let metadata = assets
        .publish_metadata(
            revision,
            sidecar,
            PublishAssetMetadata::new(
                asset_id,
                AssetMediaType::new("image/png").unwrap(),
                None,
                revision.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    metadata.add_to(&mut command).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => {
            panic!("expected committed asset setup, got {outcome:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed asset setup, got {outcome:?}")
        }
    }

    let source = content.sealed_marker_summary().unwrap();
    let begin = BeginAssetReferenceSet::new(
        AssetReferenceSetId::from_bytes([set_seed; 16]),
        source.sequential(),
    );
    let staging = begin.staging_authority();
    execute_one(
        store,
        assets.begin_reference_set(assets.revision(store).unwrap(), begin),
    );
    let build = assets
        .staged_reference_set_manifest(store, staging)
        .unwrap()
        .build_proof();
    execute_one(
        store,
        assets.append_reference_page(
            assets.revision(store).unwrap(),
            AppendAssetReferencePage::new(
                build,
                Box::from([AssetReferencePageEntry::new(marker, label, asset_id)]),
            )
            .unwrap(),
        ),
    );
    let build = assets
        .staged_reference_set_manifest(store, staging)
        .unwrap()
        .build_proof();
    let proof = build.sealed_proof().unwrap();
    execute_one(
        store,
        assets.seal_reference_set(
            assets.revision(store).unwrap(),
            SealAssetReferenceSet::new(build, source.sequential()),
        ),
    );
    execute_one(
        store,
        assets.update_owner_heads(
            assets.revision(store).unwrap(),
            UpdateAssetOwnerHeads::new(
                vec![AssetOwnerHeadUpdate::replace(owner, None, Some(proof))].into_boxed_slice(),
            )
            .unwrap(),
        ),
    );
    proof
}
