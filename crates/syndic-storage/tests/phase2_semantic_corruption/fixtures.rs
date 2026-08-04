use super::*;

pub(super) fn seed_item_projection() -> FixtureBatch {
    let mut records = seed_unreachable_turn_records();
    records.retain(|record| !matches!(record, FixtureRecord::TurnState(_)));
    let turn = SyndicTurnId::from_bytes([3; 16]);
    let item = SyndicItemId::from_bytes([6; 16]);
    let revision = ProjectionRevision::new(1).unwrap();
    let payload = ComposerPayload::new(vec![ComposerAtom::text("projected").unwrap()]).unwrap();
    let (content, content_records) = composer_content_records(&payload);
    let projection = fixture_inline_paragraph_projection(item, turn, "projected");
    let projection_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection.id(),
        projection.revision(),
    );
    records.extend(content_records);
    records.push(FixtureRecord::TurnState(
        TurnStateRecord::with_capture_frontiers(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            0,
            1,
            0,
            1,
            0,
            Some(
                TurnEndStatus::new(
                    TurnTerminalOutcome::Interrupted,
                    Some(TurnIncompleteReason::ItemAuditFailed),
                )
                .unwrap(),
            ),
            timestamp(4),
        )
        .unwrap(),
    ));
    records.push(FixtureRecord::CanonicalItem(
        CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            None,
        ),
    ));
    records.push(FixtureRecord::TurnItem(TurnItemIndexRecord::new(
        turn,
        TurnItemOrdinal::FIRST,
        item,
        revision,
    )));
    records.push(FixtureRecord::Projection(projection.clone()));
    records.push(FixtureRecord::StableItemProjection(
        StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection.id(),
            projection.revision(),
        ),
    ));
    records.push(FixtureRecord::ItemProjectionSet(
        ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            revision,
            ProjectionTextSource::composer(content),
            9,
            1,
            0,
            projection_digest,
            1,
            0,
            projection_digest,
            MarkdownParserCheckpoint::new(
                9,
                9,
                ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::new(2).unwrap()),
                9,
                Box::<str>::default(),
                false,
                None,
            ),
            true,
        ),
    ));
    records.push(FixtureRecord::ItemProjectionHead(
        ItemProjectionHeadRecord::new(
            item,
            revision,
            revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        ),
    ));
    batch(records)
}

fn seed_unreachable_turn_records() -> Vec<FixtureRecord> {
    let mut records = empty_thread_records(id(1), draft_id(2));
    let turn = SyndicTurnId::from_bytes([3; 16]);
    records.push(FixtureRecord::Turn(TurnRecord::new(
        turn,
        id(1),
        TurnKind::OrdinaryUser,
        ConversationParent::Root,
        None,
        TurnDepth::FIRST,
        root_turn_chain_digest(turn),
        timestamp(2),
    )));
    records.push(FixtureRecord::TurnState(fixture_turn_state(
        turn,
        TurnStateRevision::FIRST,
        TurnLifecycle::Interrupted,
        0,
        0,
        timestamp(2),
    )));
    records
}
