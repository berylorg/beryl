use std::num::NonZeroU64;

use beryl_home_store::{HomeCommand, SidecarByteLimit, SidecarNamespace};
use beryl_model::{
    AssetId, AssetReferenceSetId, SealedAssetReferenceSetProof, SyndicDraftId, SyndicDraftMarkerId,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadUpdate,
    AssetReferencePageEntry, BeginAssetReferenceSet, PublishAssetMetadata, SealAssetReferenceSet,
    UpdateAssetOwnerHeads,
};
use syndic_storage::ImageLabelOrdinal;

use super::{Fixture, execute_one, point_limit};

pub(super) fn admit_asset(
    fixture: &mut Fixture,
    marker_id: SyndicDraftMarkerId,
    bytes: &[u8],
    owner_draft: SyndicDraftId,
    set_seed: u8,
) -> (AssetId, SealedAssetReferenceSetProof) {
    admit_asset_at_label(
        fixture,
        marker_id,
        ImageLabelOrdinal::FIRST,
        bytes,
        owner_draft,
        set_seed,
    )
}

pub(super) fn admit_asset_at_label(
    fixture: &mut Fixture,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    bytes: &[u8],
    owner_draft: SyndicDraftId,
    set_seed: u8,
) -> (AssetId, SealedAssetReferenceSetProof) {
    let sidecar = fixture
        .store
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
    let assets = fixture.state.assets();
    let revision = assets.revision(&fixture.store).unwrap();
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
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    metadata.add_to(&mut command).unwrap();
    match fixture.store.execute(command) { beryl_home_store::CommandOutcome::Committed { later_failure: None, .. } => {}, beryl_home_store::CommandOutcome::NotCommitted { evidence } => panic!("expected committed asset admission: {evidence:?}"), outcome @ beryl_home_store::CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("unexpected later failure: {outcome:?}"), outcome @ beryl_home_store::CommandOutcome::Indeterminate { .. } => panic!("indeterminate asset admission: {outcome:?}"), }

    let proof = admit_reference_set(fixture, marker_id, label, asset_id, owner_draft, set_seed);
    (asset_id, proof)
}

pub(super) fn admit_reference_set(
    fixture: &mut Fixture,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
    owner_draft: SyndicDraftId,
    set_seed: u8,
) -> SealedAssetReferenceSetProof {
    let assets = fixture.state.assets();
    let source = fixture
        .syndic
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content()
        .sealed_marker_summary()
        .unwrap();
    let begin =
        BeginAssetReferenceSet::new(AssetReferenceSetId::from_bytes([set_seed; 16]), source);
    let staging = begin.staging_authority();
    execute_one(
        &fixture.store,
        assets.begin_reference_set(assets.revision(&fixture.store).unwrap(), begin),
    );
    let build = assets
        .staged_reference_set_manifest(&fixture.store, staging)
        .unwrap()
        .build_proof();
    execute_one(
        &fixture.store,
        assets.append_reference_page(
            assets.revision(&fixture.store).unwrap(),
            AppendAssetReferencePage::new(
                build,
                Box::from([AssetReferencePageEntry::new(marker_id, label, asset_id)]),
            )
            .unwrap(),
        ),
    );
    let build = assets
        .staged_reference_set_manifest(&fixture.store, staging)
        .unwrap()
        .build_proof();
    let proof = build.sealed_proof().unwrap();
    execute_one(
        &fixture.store,
        assets.seal_reference_set(
            assets.revision(&fixture.store).unwrap(),
            SealAssetReferenceSet::new(build, source),
        ),
    );
    execute_one(
        &fixture.store,
        assets.update_owner_heads(
            assets.revision(&fixture.store).unwrap(),
            UpdateAssetOwnerHeads::new(
                vec![AssetOwnerHeadUpdate::replace(
                    AssetOwner::CurrentDraft(owner_draft),
                    None,
                    Some(proof),
                )]
                .into_boxed_slice(),
            )
            .unwrap(),
        ),
    );
    proof
}
