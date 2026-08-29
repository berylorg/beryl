#![cfg(feature = "test-faults")]

mod support;

#[path = "phase2_semantic_corruption/fixtures.rs"]
mod fixtures;

use fixtures::seed_item_projection;

use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, ProjectionRevision,
    SyndicAcceptedInputId, SyndicDraftId, SyndicItemId, SyndicPathDigest, SyndicResourceId,
    SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_inline_paragraph_projection, fixture_item_projection_digest_seed,
};
use syndic_storage::*;

use support::semantic::{exercise_case, exercise_seeded_populated_case};
use support::*;

fn base() -> FixtureBatch {
    batch(empty_thread_records(id(1), draft_id(2)))
}

#[test]
fn draft_tail_and_turn_topology_corruption_fail_registration_verification_and_recovery() {
    exercise_case(
        "missing-draft",
        "thread current draft is missing",
        &[(id(1), draft_id(2))],
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
        &[(id(1), draft_id(2))],
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
        &[(id(1), draft_id(2))],
        base,
        || {
            let missing = SyndicTurnId::from_bytes([9; 16]);
            batch([FixtureRecord::Thread(ThreadRecord::new(
                id(1),
                SelectedPathProof::new(
                    Some(missing),
                    ThreadRevision::new(1).unwrap(),
                    SyndicPathDigest::from_bytes([7; 32]),
                ),
                draft_id(2),
                ThreadLineageProof::new(
                    None,
                    None,
                    syndic_storage::ThreadLineageDepth::FIRST,
                    syndic_storage::root_thread_lineage_digest(id(1)),
                ),
                None,
            ))])
        },
    );

    exercise_case(
        "wrong-root-digest",
        "root turn depth, ancestor skip, or chain digest is invalid",
        &[(id(1), draft_id(2))],
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

    exercise_seeded_populated_case(
        "wrong-child-ancestor-skip",
        "child turn depth, ancestor skip, or chain digest is invalid",
        |_store, _storage| {
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
        &[(thread, draft)],
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
                        None,
                        None,
                        0,
                        0,
                        0,
                    )
                    .unwrap(),
                ),
                FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                    thread,
                    ProjectionRevision::new(2).unwrap(),
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
        "accepted order does not begin at the first ordinal",
        &[(id(1), draft_id(2))],
        base,
        || {
            let input_id = SyndicAcceptedInputId::from_bytes([5; 16]);
            let ordinal = AcceptedInputOrdinal::new(2).unwrap();
            let revision = AcceptedInputRevision::new(1).unwrap();
            let generation = AcceptedRouteGeneration::FIRST;
            batch([
                FixtureRecord::AcceptedInput(
                    AcceptedInputRecord::new(
                        input_id,
                        id(1),
                        ordinal,
                        AcceptedInputAdmissionProof::new(
                            ThreadRevision::new(1).unwrap(),
                            SyndicDraftId::from_bytes(*input_id.as_bytes()),
                            DraftRevision::new(1).unwrap(),
                            InputGateRevision::new(1).unwrap(),
                            draft_id(2),
                        )
                        .unwrap(),
                        generation,
                        empty_composer_content(),
                        None,
                        timestamp(1),
                    )
                    .unwrap(),
                ),
                FixtureRecord::Thread(ThreadRecord::new(
                    id(1),
                    SelectedPathProof::new(
                        None,
                        ThreadRevision::new(2).unwrap(),
                        empty_selected_path_digest(),
                    ),
                    draft_id(2),
                    ThreadLineageProof::new(
                        None,
                        None,
                        ThreadLineageDepth::FIRST,
                        root_thread_lineage_digest(id(1)),
                    ),
                    None,
                )),
                FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                    id(1),
                    draft_id(2),
                    DraftRevision::new(1).unwrap(),
                    ThreadRevision::new(2).unwrap(),
                )),
                FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                    id(1),
                    ProjectionRevision::new(2).unwrap(),
                    ThreadRevision::new(2).unwrap(),
                    None,
                    empty_selected_path_digest(),
                    true,
                    timestamp(1),
                )),
                FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                    id(1),
                    ordinal,
                    input_id,
                    generation,
                )),
                FixtureRecord::AcceptedRouteGeneration(
                    AcceptedRouteGenerationRecord::new(
                        id(1),
                        generation,
                        AcceptedRouteRevision::FIRST,
                        AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
                        Some(AcceptedInputOrdinal::FIRST),
                        Some(ordinal),
                        2,
                        0,
                        0,
                        2,
                        0,
                        0,
                        0,
                    )
                    .unwrap(),
                ),
                FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
                    input_id,
                    id(1),
                    generation,
                    ordinal,
                    revision,
                    AcceptedRouteLeafState::NextTurn(NextTurnReason::PendingTurn),
                    AcceptedInputLifecycle::Admitted,
                )),
                FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
                    id(1),
                    generation,
                    AcceptedRouteRevision::FIRST,
                    AcceptedInputOrdinal::FIRST,
                    ordinal,
                )),
                FixtureRecord::InputGate(
                    InputGateRecord::new(
                        id(1),
                        InputGateRevision::new(2).unwrap(),
                        InputGateState::Idle,
                        2,
                        Some(generation),
                        None,
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
        &[(id(1), draft_id(2))],
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
        &[(id(1), draft_id(2))],
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
        &[(id(1), draft_id(2))],
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
                    None,
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
        &[(id(1), draft_id(2))],
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
        &[(id(1), draft_id(2))],
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
