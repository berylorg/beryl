use std::num::NonZeroU64;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeStore, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadUpdate,
    AssetReferencePageEntry, BeginAssetReferenceSet, BerylState, PublishAssetMetadata,
    SealAssetReferenceSet, UpdateAssetOwnerHeads,
};
use syndic_storage::{
    ComposerAtom, ComposerPayload, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    ImageLabelOrdinal, PreparedContent, SyndicCurrentDraft, SyndicStorage,
};

use super::{
    IMAGE_LEADING_TEXT,
    storage::{execute, point_limit, stage_prepared_content, timestamp},
};

pub(super) fn prepare_repeated_image(
    home: &HomeStore,
    storage: SyndicStorage,
    state: &BerylState,
    thread_id: SyndicThreadId,
    seed: u8,
) -> beryl_model::SealedAssetReferenceSetProof {
    let current = storage
        .current_draft(home, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let markers = [
        marker_id(current.draft().id(), 1),
        marker_id(current.draft().id(), 2),
    ];
    let label = ImageLabelOrdinal::FIRST;
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text(IMAGE_LEADING_TEXT).unwrap(),
        ComposerAtom::image_marker(markers[0], label),
        ComposerAtom::text(" between ").unwrap(),
        ComposerAtom::image_marker(markers[1], label),
        ComposerAtom::text(" after").unwrap(),
    ])
    .unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(home, storage, &prepared);
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, timestamp(7)).unwrap()
    else {
        panic!("image fixture must replace the current draft payload")
    };
    execute(
        home,
        storage.update_draft_payload(storage.revision(home).unwrap(), update),
    );
    let current = storage
        .current_draft(home, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let asset = publish_image_asset(home, state);
    seal_repeated_reference_set(home, state, &current, markers, label, asset, seed)
}

fn publish_image_asset(home: &HomeStore, state: &BerylState) -> AssetId {
    let sidecar = home
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"\x89PNG\r\n\x1a\nphase54-repeated-steering-image",
            SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = state.assets();
    let revision = assets.revision(home).unwrap();
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
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    metadata.add_to(&mut command).unwrap();
    match home.execute(command) {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("active-steering image metadata command committed with later failure: {outcome:?}"),
        CommandOutcome::NotCommitted { evidence } => panic!("active-steering image metadata command was not committed: {evidence:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("active-steering image metadata command was indeterminate: {outcome:?}"),
    }
    asset
}

fn marker_id(draft: SyndicDraftId, ordinal: u64) -> SyndicDraftMarkerId {
    let mut bytes = *draft.as_bytes();
    bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
    SyndicDraftMarkerId::from_bytes(bytes)
}

fn seal_repeated_reference_set(
    home: &HomeStore,
    state: &BerylState,
    current: &SyndicCurrentDraft,
    markers: [SyndicDraftMarkerId; 2],
    label: ImageLabelOrdinal,
    asset: AssetId,
    seed: u8,
) -> beryl_model::SealedAssetReferenceSetProof {
    let source = current.draft().content().sealed_marker_summary().unwrap();
    let assets = state.assets();
    let begin = BeginAssetReferenceSet::new(
        AssetReferenceSetId::from_bytes([seed.wrapping_add(10); 16]),
        source.sequential(),
    );
    let staging = begin.staging_authority();
    execute(
        home,
        assets.begin_reference_set(assets.revision(home).unwrap(), begin),
    );
    let build = assets
        .staged_reference_set_manifest(home, staging)
        .unwrap()
        .build_proof();
    execute(
        home,
        assets.append_reference_page(
            assets.revision(home).unwrap(),
            AppendAssetReferencePage::new(
                build,
                Box::new([
                    AssetReferencePageEntry::new(markers[0], label, asset),
                    AssetReferencePageEntry::new(markers[1], label, asset),
                ]),
            )
            .unwrap(),
        ),
    );
    let build = assets
        .staged_reference_set_manifest(home, staging)
        .unwrap()
        .build_proof();
    let ordered_assets = build.ordered_assets();
    let seal = SealAssetReferenceSet::new(build, source.sequential(), ordered_assets).unwrap();
    let proof = seal.sealed_proof();
    execute(
        home,
        assets.seal_reference_set(assets.revision(home).unwrap(), seal),
    );
    execute(
        home,
        assets.update_owner_heads(
            assets.revision(home).unwrap(),
            UpdateAssetOwnerHeads::new(
                vec![AssetOwnerHeadUpdate::replace(
                    AssetOwner::CurrentDraft(current.draft().id()),
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
