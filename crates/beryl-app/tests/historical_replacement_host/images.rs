use super::*;

use beryl_model::{ImageLabelOrdinal, SyndicDraftMarkerId, SyndicItemId};
use beryl_state::*;

#[test]
fn marker_only_import_preserves_identity_label_and_exact_leading_gap() {
    let (_home, store, state, storage, thread) =
        host_support::fixture_with_state("replacement-host-image", 201);
    let marker = SyndicDraftMarkerId::from_bytes([230; 16]);
    let label = ImageLabelOrdinal::new(7).unwrap();
    let assets = state.assets();
    let asset = publish_asset(&store, &assets, b"image-fixture");
    let proof = seal_marker_set(&store, &assets, &[(marker, label)], asset);
    let item = SyndicItemId::from_bytes([204; 16]);
    host_support::committed(host_support::execute(
        &store,
        assets.update_owner_heads(
            assets.revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::new([AssetOwnerHeadUpdate::replace(
                AssetOwner::SubmittedTurnItem(item),
                None,
                Some(proof),
            )]))
            .unwrap(),
        ),
    ));
    let payload = ComposerPayload::new(vec![ComposerAtom::image_marker(marker, label)]).unwrap();
    let target = seed_source_with_assets(&store, &storage, thread, &payload, Some(proof));
    let mut host = activate(&store, storage.clone(), thread);
    let ticket = host
        .begin_historical_replacement(&store, host.binding().unwrap(), target, operation(211))
        .unwrap();
    let completion = finish(&mut host, &store, &assets, ticket);
    assert!(matches!(
        completion.caret().unwrap().gap,
        gpui_text_input::InlineObjectGap::Before(_)
    ));
    let markers = storage
        .draft_piece_marker_demand(
            &store,
            completion.binding().root(),
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(0),
                DraftPieceMarkerDirectionV1::Forward,
                None,
                1,
                4096,
            ),
        )
        .unwrap();
    assert_eq!(markers.markers()[0].marker().marker_id(), marker);
    assert_eq!(markers.markers()[0].marker().label(), label);
    let head = assets
        .owner_head(
            &store,
            AssetOwner::CurrentDraft(completion.binding().candidate().draft_id()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(head.set(), proof);
    assert_eq!(
        assets
            .owner_head(&store, AssetOwner::SubmittedTurnItem(item))
            .unwrap()
            .unwrap()
            .set(),
        proof
    );
}
