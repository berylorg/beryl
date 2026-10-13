use super::*;
use beryl_home_store::{HomeCommand, SidecarByteLimit, SidecarNamespace};
use beryl_model::{
    AssetId, AssetReferenceSetId, ImageLabelOrdinal, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof, SequentialMarkerSummaryV1, SyndicDraftMarkerId,
};
use beryl_state::*;

pub fn publish_asset(store: &HomeStore, assets: &AssetState, bytes: &[u8]) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
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
    host_support::committed(store.execute(command));
    asset
}

pub fn seal_marker_set(
    store: &HomeStore,
    assets: &AssetState,
    markers: &[(SyndicDraftMarkerId, ImageLabelOrdinal)],
    asset: AssetId,
) -> SealedAssetReferenceSetProof {
    let authority = AssetReferenceSetStagingAuthority::new(
        AssetReferenceSetId::from_bytes([231; 16]),
        [231; 32],
    );
    host_support::committed(host_support::execute(
        store,
        assets.begin_reference_set(
            assets.revision(store).unwrap(),
            BeginAssetReferenceSet::new(authority),
        ),
    ));
    for page in markers.chunks(beryl_state::ASSET_REFERENCE_PAGE_MAX_ENTRIES) {
        let manifest = assets
            .staged_reference_set_manifest(store, authority)
            .unwrap();
        let entries = page
            .iter()
            .map(|&(marker, label)| AssetReferencePageEntry::new(marker, label, asset))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        host_support::committed(host_support::execute(
            store,
            assets.append_reference_page(
                assets.revision(store).unwrap(),
                AppendAssetReferencePage::new(manifest.build_proof(), entries).unwrap(),
            ),
        ));
    }
    let mut sequential = beryl_model::sequential_marker_digest_seed();
    let mut ordered = beryl_model::ordered_marker_asset_digest_seed();
    let mut maximum = None;
    for &(marker, label) in markers {
        sequential = beryl_model::advance_sequential_marker_digest(sequential, marker, label);
        ordered = beryl_model::advance_ordered_marker_asset_digest(ordered, marker, label, asset);
        maximum = Some(maximum.map_or(label, |old: ImageLabelOrdinal| old.max(label)));
    }
    let manifest = assets
        .staged_reference_set_manifest(store, authority)
        .unwrap();
    let seal = SealAssetReferenceSet::new(
        manifest.build_proof(),
        SequentialMarkerSummaryV1::new(sequential, markers.len() as u64, maximum).unwrap(),
        OrderedMarkerAssetSummaryV1::new(ordered, markers.len() as u64),
    )
    .unwrap();
    let proof = seal.sealed_proof();
    host_support::committed(host_support::execute(
        store,
        assets.seal_reference_set(assets.revision(store).unwrap(), seal),
    ));
    proof
}
