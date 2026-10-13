use super::*;

#[test]
fn dense_image_frames_preserve_identity_order_labels_assets_and_historical_owner() {
    let markers = (1u128..=180)
        .map(|n| {
            (
                SyndicDraftMarkerId::from_bytes(n.to_be_bytes()),
                ImageLabelOrdinal::new(n as u64).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let mut atoms = vec![ComposerAtom::text("").unwrap()];
    atoms.extend(
        markers
            .iter()
            .map(|&(id, label)| ComposerAtom::image_marker(id, label)),
    );
    atoms.push(ComposerAtom::text("\r\n🙂").unwrap());
    let f = Fixture::new(
        "dense-images",
        ComposerPayload::new(atoms).unwrap(),
        markers.clone(),
        65_536,
    );
    let historical = f
        .state
        .assets()
        .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
        .unwrap();
    let authority = f
        .storage
        .image_label_authority_head(
            &f.store,
            f.thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap();
    let p = f.prepare();
    begin(&f, &p);
    let root = advance_all(&f, &p);
    assert_eq!(text(&f, root), "\r\n🙂".as_bytes());
    assert_eq!(root.summary().marker_count(), markers.len() as u64);
    let mut cursor = None;
    let mut found = Vec::new();
    loop {
        let page = f
            .storage
            .draft_piece_marker_demand(
                &f.store,
                root,
                DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::InclusiveRange {
                        start: 0,
                        end: root.summary().logical_utf8_bytes(),
                    },
                    DraftPieceMarkerDirectionV1::Forward,
                    cursor,
                    11,
                    4096,
                ),
            )
            .unwrap();
        assert!(page.markers().len() <= 11);
        for at in page.markers() {
            let marker = at.marker();
            found.push((marker.marker_id(), marker.label()));
            assert_eq!(marker.asset_id(), f.asset.unwrap());
        }
        if page.requested_side_complete() {
            break;
        }
        cursor = page.continuation();
        assert!(cursor.is_some());
    }
    assert_eq!(found, markers);
    start(&f, &p);
    assert_eq!(
        f.state
            .assets()
            .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
            .unwrap(),
        historical
    );
    let current = f
        .state
        .assets()
        .owner_head(&f.store, AssetOwner::CurrentDraft(f.session.draft_id()))
        .unwrap()
        .unwrap();
    assert_eq!(current.set(), f.proof.unwrap());
    assert_eq!(
        f.storage
            .image_label_authority_head(
                &f.store,
                f.thread,
                SyndicPointReadLimit::new(65_536).unwrap()
            )
            .unwrap(),
        authority
    );
}

#[test]
fn empty_and_marker_only_sources_complete_without_fabricated_text_leaves() {
    for marker_only in [false, true] {
        let marker = SyndicDraftMarkerId::from_bytes([233; 16]);
        let label = ImageLabelOrdinal::new(3).unwrap();
        let (payload, markers) = if marker_only {
            (
                ComposerPayload::new(vec![ComposerAtom::image_marker(marker, label)]).unwrap(),
                vec![(marker, label)],
            )
        } else {
            (
                ComposerPayload::new(vec![
                    ComposerAtom::text("").unwrap(),
                    ComposerAtom::text("").unwrap(),
                ])
                .unwrap(),
                vec![],
            )
        };
        let f = Fixture::new("zero-text-import", payload, markers, 65_536);
        let p = f.prepare();
        begin(&f, &p);
        let root = advance_all(&f, &p);
        assert_eq!(root.summary().logical_utf8_bytes(), 0);
        assert_eq!(root.summary().marker_count(), u64::from(marker_only));
        assert!(text(&f, root).is_empty());
        let validated = f.storage.draft_piece_text_demand(
            &f.store, root, syndic_storage::DraftPieceTextDemandV1::Validate(0), 4,
        ).unwrap();
        assert_eq!(validated.start(), 0);
        assert_eq!(validated.end(), 0);
        assert!(validated.bytes().is_empty());
        assert!(f.storage.draft_piece_text_demand(
            &f.store, root, syndic_storage::DraftPieceTextDemandV1::Validate(1), 4,
        ).is_err());
        start(&f, &p);
        assert_eq!(f.head().newest_root(), root);
    }
}
