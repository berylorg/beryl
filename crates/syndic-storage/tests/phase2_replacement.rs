#![cfg(feature = "test-faults")]

mod support;

use beryl_model::{DraftRevision, ProjectionRevision, SyndicItemId, SyndicTurnId, ThreadRevision};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_advance_transcript_digest, fixture_empty_projection,
    fixture_item_projection_digest_seed, fixture_transcript_digest_seed,
};
use syndic_storage::*;

use support::populated::{
    correlate_source_user_item, populated_records, source_item, source_projection, source_turn,
};
use support::semantic::exercise_case;
use support::*;

fn root() -> SyndicTurnId {
    SyndicTurnId::from_bytes([29; 16])
}

fn selected_source() -> SelectedPathProof {
    let source = source_turn();
    let digest = child_turn_chain_digest(source, root(), root_turn_chain_digest(root()));
    SelectedPathProof::new(Some(source), ThreadRevision::new(1).unwrap(), digest)
}

fn target_entry() -> CurrentTranscriptEntryProof {
    CurrentTranscriptEntryProof::new(
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(2).unwrap(),
    )
}

fn replacement_draft(target: SyndicTurnId, selected: SelectedPathProof) -> FixtureRecord {
    FixtureRecord::Draft(DraftRecord::new(
        draft_id(31),
        id(30),
        DraftRevision::new(2).unwrap(),
        DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(
            target,
            selected,
            target_entry(),
        )),
        empty_composer_content(),
        timestamp(1),
        timestamp(4),
    ))
}

fn valid_replacement_mutation() -> FixtureBatch {
    replacement_mutation(replacement_draft(source_turn(), selected_source()))
}

fn replacement_mutation(draft: FixtureRecord) -> FixtureBatch {
    batch([
        draft,
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            id(30),
            draft_id(31),
            DraftRevision::new(2).unwrap(),
            ThreadRevision::new(1).unwrap(),
        )),
    ])
}

fn replacement_seed_records() -> Vec<FixtureRecord> {
    let turn = source_turn();
    let item = SyndicItemId::from_bytes([27; 16]);
    let revision = ProjectionRevision::new(1).unwrap();
    let item_revision = ProjectionRevision::new(4).unwrap();
    let content = empty_composer_content();
    let projection_record = fixture_empty_projection(item, turn);
    let projection = projection_record.id();
    let projection_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection,
        projection_record.revision(),
    );
    let source_entry = TranscriptViewEntryRecord::new(
        id(30),
        TranscriptGeneration::FIRST,
        TranscriptPosition::FIRST,
        source_item(),
        item_revision,
        ItemProjectionGeneration::FIRST,
        source_projection(),
        revision,
    );
    let target_entry = TranscriptViewEntryRecord::new(
        id(30),
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(2).unwrap(),
        item,
        item_revision,
        ItemProjectionGeneration::FIRST,
        projection,
        projection_record.revision(),
    );
    let transcript_digest = fixture_advance_transcript_digest(
        fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &source_entry),
        &target_entry,
    );
    let mut records = populated_records();
    records.retain(|record| {
        !matches!(record, FixtureRecord::TurnState(state) if state.turn_id() == turn)
            && !matches!(record, FixtureRecord::TranscriptViewHead(head) if head.thread_id() == id(30))
            && !matches!(record, FixtureRecord::TranscriptBuild(build)
                if build.thread_id() == id(30)
                    && build.generation() == TranscriptGeneration::FIRST)
            && !matches!(record, FixtureRecord::TranscriptPathTurn(path)
                if path.thread_id() == id(30)
                    && path.generation() == TranscriptGeneration::FIRST
                    && path.depth() == TurnDepth::new(2).unwrap())
    });
    records.extend([
        FixtureRecord::TurnState(fixture_turn_state(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            2,
            timestamp(4),
        )),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::new(2).unwrap(),
            revision,
            content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::new(2).unwrap(),
            item,
            revision,
        )),
        FixtureRecord::Projection(projection_record.clone()),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection,
            projection_record.revision(),
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            item_revision,
            ProjectionTextSource::composer(content),
            0,
            1,
            0,
            projection_digest,
            1,
            0,
            projection_digest,
            MarkdownParserCheckpoint::new(
                0,
                0,
                ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::FIRST),
                0,
                Box::<str>::default(),
                false,
                None,
            ),
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            revision,
            item_revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            id(30),
            TranscriptGeneration::FIRST,
            revision,
            2,
            Some(turn),
            selected_source().digest(),
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            id(30),
            TranscriptGeneration::FIRST,
            revision,
            ThreadRevision::new(1).unwrap(),
            Some(turn),
            selected_source().digest(),
            2,
            2,
            transcript_digest,
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            id(30),
            TranscriptGeneration::FIRST,
            TurnDepth::new(2).unwrap(),
            turn,
            selected_source().digest(),
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            2,
            2,
            timestamp(4),
        )),
        FixtureRecord::TranscriptViewEntry(target_entry),
    ]);
    correlate_source_user_item(&mut records, item, revision, content, None, timestamp(4));
    records
}

fn replacement_seed() -> FixtureBatch {
    batch(replacement_seed_records())
}

#[test]
fn replacement_intent_roundtrips_with_exact_selected_path_proof() {
    let home = TestHome::new("replacement-roundtrip");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, replacement_seed());
    commit(&store, storage, valid_replacement_mutation());
    store.validate_registered_domains().unwrap();
    let draft = storage
        .draft(
            &store,
            draft_id(31),
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let DraftSubmissionIntent::Replacement(intent) = draft.submission_intent() else {
        panic!("replacement intent did not roundtrip");
    };
    assert_eq!(intent.target_turn_id(), source_turn());
    assert_eq!(intent.selected_path(), selected_source());
    assert_eq!(intent.transcript_entry(), target_entry());
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn replacement_intent_rejects_stale_proof_and_wrong_entry_target() {
    exercise_case(
        "replacement-selected-proof",
        "replacement edit selected-path proof disagrees with current thread",
        replacement_seed,
        || {
            let selected = SelectedPathProof::new(
                Some(root()),
                ThreadRevision::new(1).unwrap(),
                root_turn_chain_digest(root()),
            );
            replacement_mutation(replacement_draft(source_turn(), selected))
        },
    );
    exercise_case(
        "replacement-off-path",
        "replacement edit transcript entry or user item disagrees",
        outside_target_seed,
        || {
            replacement_mutation(replacement_draft(
                SyndicTurnId::from_bytes([28; 16]),
                selected_source(),
            ))
        },
    );
    exercise_case(
        "replacement-empty-path",
        "replacement edit target requires a selected path",
        empty_target_seed,
        || {
            batch([FixtureRecord::Draft(DraftRecord::new(
                draft_id(2),
                id(1),
                DraftRevision::new(1).unwrap(),
                DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(
                    SyndicTurnId::from_bytes([3; 16]),
                    SelectedPathProof::new(
                        None,
                        ThreadRevision::new(1).unwrap(),
                        empty_selected_path_digest(),
                    ),
                    CurrentTranscriptEntryProof::new(
                        TranscriptGeneration::FIRST,
                        TranscriptPosition::FIRST,
                    ),
                )),
                empty_composer_content(),
                timestamp(1),
                timestamp(1),
            ))])
        },
    );
}

#[test]
fn replacement_intent_rejects_provider_operation_target() {
    let thread = id(50);
    let draft = draft_id(51);
    let target = SyndicTurnId::from_bytes([52; 16]);
    let digest = root_turn_chain_digest(target);
    exercise_case(
        "replacement-provider-operation",
        "replacement edit target is not an ordinary user turn",
        || {
            let mut records =
                thread_records_with_activity(thread, draft, Some(target), digest, timestamp(2));
            records.extend([
                FixtureRecord::Turn(TurnRecord::new(
                    target,
                    thread,
                    TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction),
                    ConversationParent::Root,
                    None,
                    TurnDepth::FIRST,
                    digest,
                    timestamp(2),
                )),
                FixtureRecord::TurnState(fixture_turn_state(
                    target,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Interrupted,
                    1,
                    0,
                    timestamp(2),
                )),
                FixtureRecord::SourceEvent(
                    SourceEventRecord::new(
                        target,
                        SourceEventSequence::FIRST,
                        None,
                        SourceEventPayload::TurnEnded(
                            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                        ),
                    )
                    .unwrap(),
                ),
            ]);
            records.extend(item_free_transcript_build_records(
                thread,
                ThreadRevision::new(1).unwrap(),
                &[(target, digest, TurnLifecycle::Interrupted, 1, timestamp(2))],
            ));
            batch(records)
        },
        || {
            let selected =
                SelectedPathProof::new(Some(target), ThreadRevision::new(1).unwrap(), digest);
            batch([
                FixtureRecord::Draft(DraftRecord::new(
                    draft,
                    thread,
                    DraftRevision::new(2).unwrap(),
                    DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(
                        target,
                        selected,
                        CurrentTranscriptEntryProof::new(
                            TranscriptGeneration::FIRST,
                            TranscriptPosition::FIRST,
                        ),
                    )),
                    empty_composer_content(),
                    timestamp(1),
                    timestamp(2),
                )),
                FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                    thread,
                    draft,
                    DraftRevision::new(2).unwrap(),
                    ThreadRevision::new(1).unwrap(),
                )),
            ])
        },
    );
}

#[test]
fn replacement_validation_uses_one_exact_current_entry_instead_of_ancestry_walk() {
    exercise_case(
        "replacement-wrong-current-entry",
        "replacement edit transcript entry or user item disagrees",
        replacement_seed,
        || {
            let draft = FixtureRecord::Draft(DraftRecord::new(
                draft_id(31),
                id(30),
                DraftRevision::new(2).unwrap(),
                DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(
                    source_turn(),
                    selected_source(),
                    CurrentTranscriptEntryProof::new(
                        TranscriptGeneration::FIRST,
                        TranscriptPosition::FIRST,
                    ),
                )),
                empty_composer_content(),
                timestamp(1),
                timestamp(4),
            ));
            replacement_mutation(draft)
        },
    );
}

fn outside_target_seed() -> FixtureBatch {
    let outside = SyndicTurnId::from_bytes([28; 16]);
    let mut records = replacement_seed_records();
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            outside,
            id(30),
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            root_turn_chain_digest(outside),
            timestamp(5),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            outside,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            0,
            0,
            timestamp(5),
        )),
    ]);
    batch(records)
}

fn empty_target_seed() -> FixtureBatch {
    let target = SyndicTurnId::from_bytes([3; 16]);
    let mut records = empty_thread_records(id(1), draft_id(2));
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            target,
            id(1),
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            root_turn_chain_digest(target),
            timestamp(2),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            target,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            0,
            0,
            timestamp(2),
        )),
    ]);
    batch(records)
}
