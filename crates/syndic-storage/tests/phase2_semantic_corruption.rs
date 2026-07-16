#![cfg(feature = "test-faults")]

mod support;

use beryl_model::{
    AcceptedInputRevision, InputGateRevision, ProjectionRevision, SyndicAcceptedInputId,
    SyndicItemId, SyndicPathDigest, SyndicResourceId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_inline_paragraph_projection, fixture_item_projection_digest_seed,
};
use syndic_storage::*;

use support::semantic::exercise_case;
use support::*;

fn base() -> FixtureBatch {
    batch(empty_thread_records(id(1), draft_id(2)))
}

#[test]
fn draft_tail_and_turn_topology_corruption_fail_registration_verification_and_recovery() {
    exercise_case(
        "missing-draft",
        "thread current draft is missing",
        base,
        || {
            let mut batch = FixtureBatch::new();
            batch.delete(FixtureDelete::Draft(draft_id(2))).unwrap();
            batch
        },
    );

    exercise_case(
        "draft-turn-collision",
        "live draft and submitted turn reuse one raw identity",
        base,
        || {
            let turn = SyndicTurnId::from_bytes(*draft_id(2).as_bytes());
            let mut batch = FixtureBatch::new();
            batch
                .put(FixtureRecord::Turn(TurnRecord::new(
                    turn,
                    id(1),
                    TurnKind::OrdinaryUser,
                    ConversationParent::Root,
                    None,
                    TurnDepth::FIRST,
                    root_turn_chain_digest(turn),
                    timestamp(2),
                )))
                .unwrap();
            batch
                .put(FixtureRecord::TurnState(fixture_turn_state(
                    turn,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Interrupted,
                    0,
                    0,
                    timestamp(2),
                )))
                .unwrap();
            batch
        },
    );

    exercise_case(
        "dangling-tail",
        "thread committed tail is missing",
        base,
        || {
            let missing = SyndicTurnId::from_bytes([9; 16]);
            batch([FixtureRecord::Thread(ThreadRecord::new(
                id(1),
                ThreadRevision::new(1).unwrap(),
                Some(missing),
                draft_id(2),
                None,
                None,
                SyndicPathDigest::from_bytes([7; 32]),
            ))])
        },
    );

    exercise_case(
        "wrong-root-digest",
        "root turn depth, ancestor skip, or chain digest is invalid",
        seed_unreachable_turn,
        || {
            let turn = SyndicTurnId::from_bytes([3; 16]);
            batch([FixtureRecord::Turn(TurnRecord::new(
                turn,
                id(1),
                TurnKind::OrdinaryUser,
                ConversationParent::Root,
                None,
                TurnDepth::FIRST,
                SyndicPathDigest::from_bytes([4; 32]),
                timestamp(2),
            ))])
        },
    );

    exercise_case(
        "wrong-child-ancestor-skip",
        "child turn depth, ancestor skip, or chain digest is invalid",
        || batch(support::populated::populated_records()),
        || {
            let root = SyndicTurnId::from_bytes([29; 16]);
            let child = support::populated::source_turn();
            let root_digest = root_turn_chain_digest(root);
            let child_digest = child_turn_chain_digest(child, root, root_digest);
            batch([FixtureRecord::Turn(TurnRecord::new(
                child,
                id(30),
                TurnKind::OrdinaryUser,
                ConversationParent::Turn(root),
                Some(child),
                TurnDepth::new(2).unwrap(),
                child_digest,
                timestamp(3),
            ))])
        },
    );
}

#[test]
fn a_thread_cannot_retain_two_blocking_turns() {
    let thread = id(60);
    let draft = draft_id(61);
    let first = SyndicTurnId::from_bytes([62; 16]);
    let second = SyndicTurnId::from_bytes([63; 16]);
    let first_digest = root_turn_chain_digest(first);
    let second_digest = child_turn_chain_digest(second, first, first_digest);
    exercise_case(
        "duplicate-blocking-turn",
        "blocking turn is not its origin thread's committed tail",
        || {
            let mut records = thread_records_with_activity(
                thread,
                draft,
                Some(first),
                first_digest,
                timestamp(2),
            );
            records.retain(|record| {
                !matches!(
                    record,
                    FixtureRecord::HistorySummary(_) | FixtureRecord::InputGate(_)
                )
            });
            records.extend([
                FixtureRecord::InputGate(
                    InputGateRecord::new(
                        thread,
                        InputGateRevision::new(1).unwrap(),
                        InputGateState::PendingTurn(first),
                        0,
                        0,
                        0,
                        0,
                    )
                    .unwrap(),
                ),
                FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                    thread,
                    ThreadRevision::new(1).unwrap(),
                    Some(first),
                    first_digest,
                    false,
                    timestamp(2),
                )),
                FixtureRecord::Turn(TurnRecord::new(
                    first,
                    thread,
                    TurnKind::OrdinaryUser,
                    ConversationParent::Root,
                    None,
                    TurnDepth::FIRST,
                    first_digest,
                    timestamp(2),
                )),
                FixtureRecord::TurnState(fixture_turn_state(
                    first,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Pending,
                    0,
                    0,
                    timestamp(2),
                )),
            ]);
            records.extend(item_free_transcript_build_records(
                thread,
                ThreadRevision::new(1).unwrap(),
                &[(first, first_digest, TurnLifecycle::Pending, 0, timestamp(2))],
            ));
            batch(records)
        },
        || {
            batch([
                FixtureRecord::Turn(TurnRecord::new(
                    second,
                    thread,
                    TurnKind::OrdinaryUser,
                    ConversationParent::Turn(first),
                    Some(first),
                    TurnDepth::new(2).unwrap(),
                    second_digest,
                    timestamp(3),
                )),
                FixtureRecord::TurnState(fixture_turn_state(
                    second,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Pending,
                    0,
                    0,
                    timestamp(3),
                )),
                FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                    first,
                    second,
                    TurnDepth::new(2).unwrap(),
                    second_digest,
                )),
            ])
        },
    );
}

fn seed_unreachable_turn() -> FixtureBatch {
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
    batch(records)
}

#[test]
fn ordering_event_item_and_projection_corruption_fail_closed() {
    exercise_case(
        "accepted-order-gap",
        "accepted-input order is not contiguous",
        base,
        || {
            let input_id = SyndicAcceptedInputId::from_bytes([5; 16]);
            let ordinal = AcceptedInputOrdinal::new(2).unwrap();
            let revision = AcceptedInputRevision::new(1).unwrap();
            batch([
                FixtureRecord::AcceptedInput(AcceptedInputRecord::new(
                    input_id,
                    id(1),
                    revision,
                    ordinal,
                    InputGateRevision::new(2).unwrap(),
                    AcceptedInputDisposition::NextTurn(NextTurnReason::PendingTurn),
                    AcceptedInputLifecycle::Admitted,
                    empty_composer_content(),
                    0,
                    timestamp(3),
                )),
                FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                    id(1),
                    ordinal,
                    input_id,
                    revision,
                )),
                FixtureRecord::AcceptedNextTurn(AcceptedNextTurnIndexRecord::new(
                    id(1),
                    ordinal,
                    input_id,
                    revision,
                )),
                FixtureRecord::InputGate(
                    InputGateRecord::new(
                        id(1),
                        InputGateRevision::new(2).unwrap(),
                        InputGateState::Idle,
                        2,
                        0,
                        1,
                        0,
                    )
                    .unwrap(),
                ),
            ])
        },
    );

    exercise_case(
        "source-event-gap",
        "source-event key or contiguous sequence disagrees",
        seed_unreachable_turn,
        || {
            let turn = SyndicTurnId::from_bytes([3; 16]);
            let sequence = SourceEventSequence::new(2).unwrap();
            batch([
                FixtureRecord::TurnState(fixture_turn_state(
                    turn,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Interrupted,
                    1,
                    0,
                    timestamp(4),
                )),
                FixtureRecord::SourceEvent(
                    SourceEventRecord::new(
                        turn,
                        sequence,
                        None,
                        syndic_storage::SourceEventPayload::TurnEnded(
                            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                        ),
                    )
                    .unwrap(),
                ),
            ])
        },
    );

    exercise_case(
        "successful-completion-without-source",
        "successful turn completion lacks exact source authority",
        seed_unreachable_turn,
        || {
            let turn = SyndicTurnId::from_bytes([3; 16]);
            batch([FixtureRecord::TurnState(fixture_turn_state(
                turn,
                TurnStateRevision::FIRST,
                TurnLifecycle::Complete,
                0,
                0,
                timestamp(4),
            ))])
        },
    );

    exercise_case(
        "turn-item-index",
        "turn-item index disagrees",
        seed_unreachable_turn,
        || {
            let turn = SyndicTurnId::from_bytes([3; 16]);
            let item = SyndicItemId::from_bytes([6; 16]);
            let wrong = SyndicItemId::from_bytes([7; 16]);
            let revision = ProjectionRevision::new(1).unwrap();
            batch([
                FixtureRecord::TurnState(fixture_turn_state(
                    turn,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Interrupted,
                    0,
                    1,
                    timestamp(4),
                )),
                FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
                    item,
                    turn,
                    TurnItemOrdinal::FIRST,
                    revision,
                    empty_composer_content(),
                    0,
                )),
                FixtureRecord::TurnItem(TurnItemIndexRecord::new(
                    turn,
                    TurnItemOrdinal::FIRST,
                    wrong,
                    revision,
                )),
            ])
        },
    );

    exercise_case(
        "projection-index",
        "stable item-projection target disagrees",
        seed_item_projection,
        || {
            let item = SyndicItemId::from_bytes([6; 16]);
            let projection = fixture_inline_paragraph_projection(
                item,
                SyndicTurnId::from_bytes([3; 16]),
                "projected",
            );
            batch([FixtureRecord::StableItemProjection(
                StableItemProjectionIndexRecord::new(
                    item,
                    ProjectionOrdinal::FIRST,
                    projection.id(),
                    ProjectionRevision::new(2).unwrap(),
                ),
            )])
        },
    );

    exercise_case(
        "missing-resource-metadata",
        "projection resource metadata is missing",
        seed_item_projection,
        || {
            let projection = fixture_inline_paragraph_projection(
                SyndicItemId::from_bytes([6; 16]),
                SyndicTurnId::from_bytes([3; 16]),
                "projected",
            )
            .id();
            batch([FixtureRecord::Projection(ProjectionRecord::new(
                projection,
                ProjectionRevision::new(1).unwrap(),
                SyndicItemId::from_bytes([6; 16]),
                SyndicTurnId::from_bytes([3; 16]),
                ProjectionOrdinal::FIRST,
                ProjectionPayload::resource_reference(
                    MarkdownBlockId::from_bytes([8; 32]),
                    MarkdownBlockKind::FencedCode,
                    ProjectionSourceRange::new(0, 9).unwrap(),
                    SyndicResourceId::from_bytes([9; 16]),
                    "projected",
                )
                .unwrap(),
            ))])
        },
    );
}

fn seed_item_projection() -> FixtureBatch {
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
            0,
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
            content,
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
                ContentPieceOrdinal::new(2).unwrap(),
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
