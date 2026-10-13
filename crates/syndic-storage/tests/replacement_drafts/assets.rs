use super::*;
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetReferencePageEntry,
    AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, PublishAssetMetadata,
    SealAssetReferenceSet,
};
use std::num::NonZeroU64;

pub(super) fn publish_asset(store: &HomeStore, state: &BerylState, bytes: &[u8]) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            beryl_home_store::SidecarNamespace::new("images").unwrap(),
            bytes,
            beryl_home_store::SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let revision = state.assets().revision(store).unwrap();
    let p = state
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
    p.add_to(&mut command).unwrap();
    committed(store.execute(command));
    asset
}
pub(super) fn seal_set(
    store: &HomeStore,
    state: &BerylState,
    markers: &[(SyndicDraftMarkerId, ImageLabelOrdinal)],
    asset: AssetId,
) -> SealedAssetReferenceSetProof {
    let authority = AssetReferenceSetStagingAuthority::new(
        AssetReferenceSetId::from_bytes([210; 16]),
        [210; 32],
    );
    committed(execute(
        store,
        state.assets().begin_reference_set(
            state.assets().revision(store).unwrap(),
            BeginAssetReferenceSet::new(authority),
        ),
    ));
    for page in markers.chunks(beryl_state::ASSET_REFERENCE_PAGE_MAX_ENTRIES) {
        let manifest = state
            .assets()
            .staged_reference_set_manifest(store, authority)
            .unwrap();
        let entries = page
            .iter()
            .map(|(id, label)| AssetReferencePageEntry::new(*id, *label, asset))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        committed(execute(
            store,
            state.assets().append_reference_page(
                state.assets().revision(store).unwrap(),
                AppendAssetReferencePage::new(manifest.build_proof(), entries).unwrap(),
            ),
        ));
    }
    let mut sequential = beryl_model::sequential_marker_digest_seed();
    let mut ordered = beryl_model::ordered_marker_asset_digest_seed();
    let mut maximum = None;
    for &(id, label) in markers {
        sequential = beryl_model::advance_sequential_marker_digest(sequential, id, label);
        ordered = beryl_model::advance_ordered_marker_asset_digest(ordered, id, label, asset);
        maximum = Some(maximum.map_or(label, |old: ImageLabelOrdinal| old.max(label)));
    }
    let manifest = state
        .assets()
        .staged_reference_set_manifest(store, authority)
        .unwrap();
    let seal = SealAssetReferenceSet::new(
        manifest.build_proof(),
        SequentialMarkerSummaryV1::new(sequential, markers.len() as u64, maximum).unwrap(),
        OrderedMarkerAssetSummaryV1::new(ordered, markers.len() as u64),
    )
    .unwrap();
    let proof = seal.sealed_proof();
    committed(execute(
        store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(store).unwrap(), seal),
    ));
    proof
}
