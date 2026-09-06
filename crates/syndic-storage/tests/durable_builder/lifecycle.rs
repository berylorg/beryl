#[test]
fn cancellation_during_an_effect_and_between_effects_preserves_candidate_state() {
    for (case, seed, cancel_between) in [
        ("cancel-active-effect", 200, false),
        ("cancel-between-effects", 210, true),
    ] {
        let (home, store, storage, thread) = fixture(case, seed);
        let mut store = store;
        let mut storage = storage;
        let current = current(&storage, &store, thread);
        let mut session = open_session(&storage, &store, &current, seed + 1, seed + 2);
        session = complete_staged(
            &storage,
            &store,
            &session,
            seed + 3,
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("abc".to_owned())],
            ),
            DraftLogicalExtentV1::new(3, 1),
        );
        let before = session.clone();
        let first = marker(seed + 4, 7, 9);
        let second = marker(seed + 5, 8, 10);
        let effect = |anchor, marker| {
            DraftPieceReplacementV1::new(
                point(anchor),
                point(anchor),
                vec![DraftPieceV1::Marker(marker)],
            )
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    anchor,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            ))
        };
        let (prepared, identity, fragments) = stage_interleaved_replacements(
            &storage,
            &store,
            &session,
            seed + 6,
            vec![effect(1, first), effect(2, second)],
            DraftLogicalExtentV1::new(3, 1),
            point(0),
        );
        loop {
            let build = open_build_fragments(&storage, &store, &prepared, &fragments);
            let reached = if cancel_between {
                build.frontier()
                    == (DraftPieceBuildFrontierV1::Planning {
                        fragment_ordinal: 2,
                    })
                    && build.marker_effect_continuation().active().is_none()
            } else {
                build.marker_effect_continuation().active().is_some()
            };
            if reached {
                break;
            }
            let advance = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    identity.draft_id(),
                    identity.session_id(),
                    identity.operation_id().as_piece_operation(),
                )
                .unwrap()
                .unwrap();
            committed(execute(
                &store,
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
            ));
        }
        drop(store);
        store = HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
        storage = SyndicStorage::register(&mut store).unwrap();
        let reopened = open_build_fragments(&storage, &store, &prepared, &fragments);
        if cancel_between {
            assert_eq!(
                reopened.frontier(),
                DraftPieceBuildFrontierV1::Planning {
                    fragment_ordinal: 2,
                }
            );
            assert!(reopened.marker_effect_continuation().active().is_none());
        } else {
            assert!(reopened.marker_effect_continuation().active().is_some());
        }
        let reopened_session =
            active_session(&storage, &store, session.draft_id(), session.session_id());
        assert_eq!(reopened_session.newest_root(), before.newest_root());
        assert_eq!(reopened_session.newest_history(), before.newest_history());
        committed(execute(
            &store,
            storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared.clone()),
        ));
        session = active_session(&storage, &store, session.draft_id(), session.session_id());
        assert_eq!(
            session.newest_candidate_generation(),
            before.newest_candidate_generation()
        );
        assert_eq!(session.newest_root(), before.newest_root());
        assert_eq!(session.newest_history(), before.newest_history());
        assert_eq!(session.logical_extent(), before.logical_extent());
        assert_eq!(
            session.newest_root().summary().marker_count(),
            before.newest_root().summary().marker_count()
        );
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &prepared, 1, &fragments)
                .is_ok()
        );
    }
}
