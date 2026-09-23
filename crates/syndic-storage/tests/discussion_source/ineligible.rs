use super::*;

#[test]
fn active_unknown_terminal_and_unfinalized_sources_are_ineligible() {
    for lifecycle in [
        TurnLifecycle::Active,
        TurnLifecycle::UnknownTerminal,
        TurnLifecycle::Interrupted,
    ] {
        let fixture = Fixture::new();
        let (source, path, entry) = fixture.selection();
        let state = fixture
            .storage
            .turn_state(&fixture.store, source.turn_id(), limit())
            .unwrap()
            .unwrap();
        let replacement = support::fixture_turn_state_with_finalization(
            source.turn_id(),
            state.revision().checked_next().unwrap(),
            lifecycle,
            state.source_event_count(),
            state.item_count(),
            0,
            state.updated_at(),
        );
        commit(
            &fixture.store,
            fixture.storage.clone(),
            batch([FixtureRecord::TurnState(replacement)]),
        );
        assert!(
            fixture
                .storage
                .prepare_discussion_source(
                    &fixture.store,
                    source,
                    path,
                    entry,
                    DiscussionContextText::new("assistant").unwrap()
                )
                .is_err()
        );
    }
}

#[test]
fn user_items_are_ineligible_even_with_matching_transcript_coordinates() {
    let fixture = Fixture::new();
    let (source, path, entry) = fixture.selection();
    let item = fixture
        .storage
        .canonical_item(&fixture.store, source.item_id(), limit())
        .unwrap()
        .unwrap();
    let (content, mut records) = support::composer_content_records(
        &ComposerPayload::new(vec![ComposerAtom::text("assistant").unwrap()]).unwrap(),
    );
    records.push(FixtureRecord::CanonicalItem(
        CanonicalItemRecord::local_user_input(
            item.id(),
            item.turn_id(),
            item.ordinal(),
            item.revision(),
            content,
            None,
        ),
    ));
    commit(&fixture.store, fixture.storage.clone(), batch(records));
    assert!(
        fixture
            .storage
            .prepare_discussion_source(
                &fixture.store,
                source,
                path,
                entry,
                DiscussionContextText::new("assistant").unwrap()
            )
            .is_err()
    );
}

#[test]
fn transcript_entry_cannot_substitute_for_current_projection_membership() {
    let fixture = Fixture::new();
    let (source, path, entry) = fixture.selection();
    let projection = fixture
        .storage
        .projection(&fixture.store, source.projection_id(), limit())
        .unwrap()
        .unwrap();
    let head = fixture
        .storage
        .item_projection_head(&fixture.store, source.item_id(), limit())
        .unwrap()
        .unwrap();
    let set = fixture
        .storage
        .item_projection_set(&fixture.store, source.item_id(), head.generation(), limit())
        .unwrap()
        .unwrap();
    assert!(projection.ordinal().get() <= set.stable_projection_count());
    commit(
        &fixture.store,
        fixture.storage.clone(),
        batch([FixtureRecord::StableItemProjection(
            StableItemProjectionIndexRecord::new(
                source.item_id(),
                projection.ordinal(),
                beryl_model::SyndicProjectionId::from_bytes([250; 16]),
                source.projection_revision(),
            ),
        )]),
    );
    assert!(
        fixture
            .storage
            .prepare_discussion_source(
                &fixture.store,
                source,
                path,
                entry,
                DiscussionContextText::new("assistant").unwrap()
            )
            .is_err()
    );
}

#[test]
fn stale_projection_heads_and_selected_away_sources_are_ineligible() {
    for selected_away in [false, true] {
        let fixture = Fixture::new();
        let (source, path, entry) = fixture.selection();
        let record = if selected_away {
            let thread = fixture
                .storage
                .thread(&fixture.store, id(30), limit())
                .unwrap()
                .unwrap();
            FixtureRecord::Thread(ThreadRecord::new(
                thread.id(),
                SelectedPathProof::new(
                    None,
                    thread.revision().checked_next().unwrap(),
                    empty_selected_path_digest(),
                ),
                thread.current_draft_id(),
                thread.lineage(),
                thread.context_owner_id(),
            ))
        } else {
            let head = fixture
                .storage
                .item_projection_head(&fixture.store, source.item_id(), limit())
                .unwrap()
                .unwrap();
            FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
                head.item_id(),
                head.revision(),
                head.source_item_revision(),
                head.generation(),
                ProjectionLifecycle::Stale,
            ))
        };
        commit(&fixture.store, fixture.storage.clone(), batch([record]));
        assert!(
            fixture
                .storage
                .prepare_discussion_source(
                    &fixture.store,
                    source,
                    path,
                    entry,
                    DiscussionContextText::new("assistant").unwrap()
                )
                .is_err()
        );
    }
}
