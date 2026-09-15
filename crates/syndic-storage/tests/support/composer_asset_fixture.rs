use std::num::NonZeroU64;

use beryl_home_store::{HomeCommand, HomeStore, SidecarByteLimit, SidecarNamespace};
use beryl_model::{AssetId, AssetReferenceSetId, SealedAssetReferenceSetProof, SyndicDraftId};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadUpdate,
    AssetReferencePageEntry, AssetState, BeginAssetReferenceSet, PublishAssetMetadata,
    SealAssetReferenceSet, UpdateAssetOwnerHeads,
};
use syndic_storage::{DraftMarkerSealProofV1, DraftPieceMarkerV1};

use super::{committed, execute};

pub fn publish_asset(store: &HomeStore, assets: &AssetState) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            &[64],
            SidecarByteLimit::new(NonZeroU64::MIN),
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
    committed(store.execute(command));
    asset_id
}

pub fn admit_draft_set(
    store: &HomeStore,
    assets: &AssetState,
    marker: DraftPieceMarkerV1,
    source: DraftMarkerSealProofV1,
    draft: SyndicDraftId,
) -> SealedAssetReferenceSetProof {
    let begin = BeginAssetReferenceSet::new(beryl_state::AssetReferenceSetStagingAuthority::new(
        AssetReferenceSetId::from_bytes([67; 16]),
        [68; 32],
    ));
    let staging = begin.staging_authority();
    committed(execute(
        store,
        assets.begin_reference_set(assets.revision(store).unwrap(), begin),
    ));
    let build = assets
        .staged_reference_set_manifest(store, staging)
        .unwrap()
        .build_proof();
    committed(execute(
        store,
        assets.append_reference_page(
            assets.revision(store).unwrap(),
            AppendAssetReferencePage::new(
                build,
                Box::from([AssetReferencePageEntry::new(
                    marker.marker_id(),
                    marker.label(),
                    marker.asset_id(),
                )]),
            )
            .unwrap(),
        ),
    ));
    let build = assets
        .staged_reference_set_manifest(store, staging)
        .unwrap()
        .build_proof();
    assert_eq!(build.ordered_assets(), source.ordered_assets());
    let seal =
        SealAssetReferenceSet::new(build, source.sequential(), source.ordered_assets()).unwrap();
    let proof = seal.sealed_proof();
    committed(execute(
        store,
        assets.seal_reference_set(assets.revision(store).unwrap(), seal),
    ));
    committed(execute(
        store,
        assets.update_owner_heads(
            assets.revision(store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                AssetOwner::CurrentDraft(draft),
                None,
                Some(proof),
            )]))
            .unwrap(),
        ),
    ));
    proof
}
