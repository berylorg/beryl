#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{DomainRegistrationError, DomainValidationError, HomeRecoveryError};
use beryl_model::{
    BindingRevision, DiscussionContextOwnerId, DraftRevision, InputGateRevision,
    ProjectionRevision, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_advance_transcript_digest, fixture_inline_paragraph_projection,
    fixture_item_projection_digest_seed, fixture_transcript_digest_seed,
};
use syndic_storage::*;

use support::populated::{
    correlate_source_user_item, populated_records, source_cas_authority, source_item,
    source_projection, source_resource_projection, source_turn,
};
use support::{
    TestHome, batch, commit, composer_content_records, draft_id, empty_composer_content,
    fixture_turn_state, id, open, timestamp,
};

fn source_digest() -> beryl_model::SyndicPathDigest {
    let root = SyndicTurnId::from_bytes([29; 16]);
    child_turn_chain_digest(source_turn(), root, root_turn_chain_digest(root))
}

fn source_transcript_digest() -> [u8; 32] {
    let entry = TranscriptViewEntryRecord::new(
        id(30),
        TranscriptGeneration::FIRST,
        TranscriptPosition::FIRST,
        source_item(),
        ProjectionRevision::new(1).unwrap(),
        ItemProjectionGeneration::FIRST,
        source_projection(),
        ProjectionRevision::new(1).unwrap(),
    );
    fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &entry)
}

fn context_source_user_item_mutation() -> FixtureBatch {
    let thread = id(30);
    let turn = source_turn();
    let item = beryl_model::SyndicItemId::from_bytes([70; 16]);
    let projection = fixture_inline_paragraph_projection(item, turn, "user");
    let revision = projection.revision();
    let (content, mut records) = composer_content_records(
        &ComposerPayload::new(vec![ComposerAtom::text("user").unwrap()]).unwrap(),
    );
    let item_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection.id(),
        revision,
    );
    let checkpoint = MarkdownParserCheckpoint::new(
        4,
        4,
        ContentPieceOrdinal::new(2).unwrap(),
        4,
        Box::<str>::default(),
        false,
        None,
    );
    let entry = TranscriptViewEntryRecord::new(
        thread,
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(2).unwrap(),
        item,
        revision,
        ItemProjectionGeneration::FIRST,
        projection.id(),
        revision,
    );
    let transcript_digest = fixture_advance_transcript_digest(source_transcript_digest(), &entry);
    let context_source = DiscussionContextSource::new(
        thread,
        turn,
        item,
        projection.id(),
        revision,
        DiscussionContextRange::new(0, 4).unwrap(),
    );
    let context = DiscussionContextEnvelope::new(
        context_source,
        DiscussionContextText::new("user").unwrap(),
        timestamp(5),
    )
    .unwrap();
    records.extend([
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::new(2).unwrap(),
            revision,
            content,
            0,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::new(2).unwrap(),
            item,
            revision,
        )),
        FixtureRecord::Projection(projection),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            fixture_inline_paragraph_projection(item, turn, "user").id(),
            revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            revision,
            content,
            4,
            1,
            0,
            item_digest,
            1,
            0,
            item_digest,
            checkpoint,
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            revision,
            revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            2,
            timestamp(4),
        )),
        FixtureRecord::TranscriptViewEntry(entry),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            revision,
            2,
            Some(turn),
            source_digest(),
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            revision,
            ThreadRevision::new(1).unwrap(),
            Some(turn),
            source_digest(),
            2,
            2,
            transcript_digest,
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TurnDepth::new(2).unwrap(),
            turn,
            source_digest(),
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            2,
            2,
            timestamp(4),
        )),
        FixtureRecord::ContextEnvelope(ContextEnvelopeRecord::new(
            DiscussionContextOwnerId::Draft(draft_id(37)),
            ContextEnvelopeRevision::FIRST,
            context,
        )),
    ]);
    correlate_source_user_item(&mut records, item, revision, content, 0, timestamp(4));
    batch(records)
}

fn context_record(owner: DiscussionContextOwnerId) -> ContextEnvelopeRecord {
    context_record_with_text(owner, "assistant")
}

fn context_record_with_text(owner: DiscussionContextOwnerId, text: &str) -> ContextEnvelopeRecord {
    context_record_with_projection(owner, source_projection(), text)
}

fn context_record_with_projection(
    owner: DiscussionContextOwnerId,
    projection: beryl_model::SyndicProjectionId,
    text: &str,
) -> ContextEnvelopeRecord {
    context_record_with_projection_revision(
        owner,
        projection,
        ProjectionRevision::new(1).unwrap(),
        text,
    )
}

fn context_record_with_projection_revision(
    owner: DiscussionContextOwnerId,
    projection: beryl_model::SyndicProjectionId,
    revision: ProjectionRevision,
    text: &str,
) -> ContextEnvelopeRecord {
    let source = DiscussionContextSource::new(
        id(30),
        source_turn(),
        source_item(),
        projection,
        revision,
        DiscussionContextRange::new(0, 9).unwrap(),
    );
    let envelope = DiscussionContextEnvelope::new(
        source,
        DiscussionContextText::new(text).unwrap(),
        timestamp(5),
    )
    .unwrap();
    ContextEnvelopeRecord::new(owner, ContextEnvelopeRevision::FIRST, envelope)
}

fn assert_context_rejection(name: &str, expected: &str, mutation: FixtureBatch) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    store.validate_registered_domains().unwrap();
    commit(&store, storage, mutation);
    match store.validate_registered_domains().unwrap_err() {
        DomainValidationError::Rejected { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected context semantic rejection, got {other:?}"),
    }
    match store.recover_same_home().unwrap_err() {
        HomeRecoveryError::DomainValidation(DomainValidationError::Rejected { domain, source }) => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected context recovery rejection, got {other:?}"),
    }
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("corrupt context reopened successfully"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(source.to_string(), expected);
        }
        other => panic!("expected context registration rejection, got {other:?}"),
    }
    reopened.close().unwrap();
}

fn validate_and_reopen(name: &str, mutation: FixtureBatch) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    store.validate_registered_domains().unwrap();
    commit(&store, storage, mutation);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn context_remains_valid_after_source_thread_selects_away_from_source_turn() {
    let thread = id(30);
    let draft = draft_id(31);
    let root = SyndicTurnId::from_bytes([29; 16]);
    let digest = root_turn_chain_digest(root);
    let thread_revision = ThreadRevision::new(2).unwrap();
    let draft_revision = DraftRevision::new(2).unwrap();
    let projection_revision = ProjectionRevision::new(2).unwrap();
    let binding_revision = BindingRevision::new(5).unwrap();
    let selected = SelectedPathProof::new(Some(root), thread_revision, digest);
    let mutation = batch([
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            thread_revision,
            Some(root),
            draft,
            None,
            None,
            digest,
        )),
        FixtureRecord::Draft(DraftRecord::new(
            draft,
            thread,
            draft_revision,
            ConversationParent::Turn(root),
            None,
            None,
            empty_composer_content(),
            timestamp(1),
            timestamp(1),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::new(2).unwrap(),
            projection_revision,
            0,
            Some(root),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::new(2).unwrap(),
            projection_revision,
            thread_revision,
            Some(root),
            digest,
            1,
            0,
            fixture_transcript_digest_seed(),
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::new(2).unwrap(),
            TurnDepth::FIRST,
            root,
            digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            0,
            timestamp(2),
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            thread_revision,
            Some(root),
            digest,
            true,
            timestamp(2),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            selected,
            BindingState::unbound("source moved selection").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        )),
    ]);
    validate_and_reopen("context-source-moved", mutation);
}

#[test]
fn submitted_context_owner_need_not_remain_on_child_selected_path() {
    let source = source_turn();
    let child_thread = id(36);
    let old_draft = draft_id(37);
    let new_draft = draft_id(38);
    let owner_turn = old_draft.submitted_turn_id();
    let alternate = SyndicTurnId::from_bytes([39; 16]);
    let source_digest = child_turn_chain_digest(
        source,
        SyndicTurnId::from_bytes([29; 16]),
        root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
    );
    let owner_digest = child_turn_chain_digest(owner_turn, source, source_digest);
    let alternate_digest = child_turn_chain_digest(alternate, source, source_digest);
    let owner = DiscussionContextOwnerId::SubmittedTurn(owner_turn);
    let thread_revision = ThreadRevision::new(2).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let head_revision = ProjectionRevision::new(2).unwrap();
    let binding_revision = BindingRevision::new(2).unwrap();
    let selected = SelectedPathProof::new(Some(alternate), thread_revision, alternate_digest);
    let generation = TranscriptGeneration::new(2).unwrap();
    let transcript_entry = TranscriptViewEntryRecord::new(
        child_thread,
        generation,
        TranscriptPosition::FIRST,
        source_item(),
        ProjectionRevision::new(1).unwrap(),
        ItemProjectionGeneration::FIRST,
        source_projection(),
        ProjectionRevision::new(1).unwrap(),
    );
    let transcript_digest =
        fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &transcript_entry);

    let mut mutation = batch([
        FixtureRecord::Thread(ThreadRecord::new(
            child_thread,
            thread_revision,
            Some(alternate),
            new_draft,
            Some(id(30)),
            Some(owner),
            alternate_digest,
        )),
        FixtureRecord::Draft(DraftRecord::new(
            new_draft,
            child_thread,
            draft_revision,
            ConversationParent::Turn(alternate),
            None,
            None,
            empty_composer_content(),
            timestamp(8),
            timestamp(8),
        )),
        FixtureRecord::ContextEnvelope(context_record(owner)),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            child_thread,
            new_draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::ThreadParent(ThreadParentIndexRecord::new(
            id(30),
            child_thread,
            thread_revision,
            owner,
        )),
        FixtureRecord::Turn(TurnRecord::new(
            owner_turn,
            child_thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(source),
            Some(source),
            TurnDepth::new(3).unwrap(),
            owner_digest,
            timestamp(6),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            owner_turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            0,
            0,
            timestamp(6),
        )),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            source,
            owner_turn,
            TurnDepth::new(3).unwrap(),
            owner_digest,
        )),
        FixtureRecord::Turn(TurnRecord::new(
            alternate,
            child_thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(source),
            Some(source),
            TurnDepth::new(3).unwrap(),
            alternate_digest,
            timestamp(7),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            alternate,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            timestamp(7),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                alternate,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            source,
            alternate,
            TurnDepth::new(3).unwrap(),
            alternate_digest,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            child_thread,
            generation,
            head_revision,
            1,
            Some(alternate),
            alternate_digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            child_thread,
            generation,
            head_revision,
            thread_revision,
            Some(alternate),
            alternate_digest,
            3,
            1,
            transcript_digest,
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            child_thread,
            generation,
            TurnDepth::FIRST,
            SyndicTurnId::from_bytes([29; 16]),
            root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            0,
            timestamp(2),
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            child_thread,
            generation,
            TurnDepth::new(2).unwrap(),
            source,
            source_digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            5,
            1,
            1,
            timestamp(4),
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            child_thread,
            generation,
            TurnDepth::new(3).unwrap(),
            alternate,
            alternate_digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            0,
            timestamp(7),
        )),
        FixtureRecord::TranscriptViewEntry(transcript_entry),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            child_thread,
            thread_revision,
            Some(alternate),
            alternate_digest,
            true,
            timestamp(8),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            child_thread,
            binding_revision,
            selected,
            BindingState::unbound("submitted context owner retained").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            child_thread,
            binding_revision,
            BindingLifecycle::Unbound,
            alternate_digest,
        )),
    ]);
    mutation
        .delete(FixtureDelete::Draft(old_draft))
        .unwrap()
        .delete(FixtureDelete::ContextEnvelope(
            DiscussionContextOwnerId::Draft(old_draft),
        ))
        .unwrap();
    validate_and_reopen("context-owner-off-later-path", mutation);
}

#[test]
fn context_reopen_checks_finalization_kind_association_text_and_owner_parent() {
    assert_context_rejection(
        "context-source-not-terminal",
        "context source turn is not finalized terminal history",
        batch([
            FixtureRecord::InputGate(
                InputGateRecord::new(
                    id(30),
                    InputGateRevision::new(1).unwrap(),
                    InputGateState::PendingTurn(source_turn()),
                    0,
                    0,
                    0,
                    0,
                )
                .unwrap(),
            ),
            FixtureRecord::TurnState(fixture_turn_state(
                source_turn(),
                TurnStateRevision::FIRST,
                TurnLifecycle::UnknownTerminal,
                5,
                1,
                timestamp(4),
            )),
            FixtureRecord::SourceEvent(
                SourceEventRecord::new(
                    source_turn(),
                    SourceEventSequence::new(5).unwrap(),
                    Some(source_cas_authority()),
                    SourceEventPayload::TurnEnded(
                        TurnEndStatus::new(TurnTerminalOutcome::UnknownTerminal, None).unwrap(),
                    ),
                )
                .unwrap(),
            ),
            FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
                id(30),
                TranscriptGeneration::FIRST,
                ProjectionRevision::new(1).unwrap(),
                ThreadRevision::new(1).unwrap(),
                Some(source_turn()),
                source_digest(),
                2,
                1,
                source_transcript_digest(),
                false,
                TranscriptBuildPhase::Complete,
            )),
            FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
                id(30),
                TranscriptGeneration::FIRST,
                TurnDepth::new(2).unwrap(),
                source_turn(),
                source_digest(),
                TurnStateRevision::FIRST,
                TurnLifecycle::UnknownTerminal,
                5,
                1,
                1,
                timestamp(4),
            )),
            FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                id(30),
                ThreadRevision::new(1).unwrap(),
                Some(source_turn()),
                source_digest(),
                false,
                timestamp(4),
            )),
        ]),
    );
    assert_context_rejection(
        "context-projection-not-current",
        "context source projection is outside its current item set",
        batch([FixtureRecord::ContextEnvelope(
            context_record_with_projection(
                DiscussionContextOwnerId::Draft(draft_id(37)),
                source_resource_projection(),
                "assistant",
            ),
        )]),
    );
    assert_context_rejection(
        "context-source-not-assistant",
        "context source item is not an assistant message",
        context_source_user_item_mutation(),
    );
    assert_context_rejection(
        "context-source-revision",
        "context source records disagree",
        batch([FixtureRecord::ContextEnvelope(
            context_record_with_projection_revision(
                DiscussionContextOwnerId::Draft(draft_id(37)),
                source_projection(),
                ProjectionRevision::new(2).unwrap(),
                "assistant",
            ),
        )]),
    );
    assert_context_rejection(
        "context-source-text",
        "context source range and exact text disagree",
        batch([FixtureRecord::ContextEnvelope(context_record_with_text(
            DiscussionContextOwnerId::Draft(draft_id(37)),
            "assistAnt",
        ))]),
    );
    let owner = DiscussionContextOwnerId::Draft(draft_id(37));
    assert_context_rejection(
        "context-owner-parent",
        "context owner parent and source turn disagree",
        batch([FixtureRecord::Draft(DraftRecord::new(
            draft_id(37),
            id(36),
            DraftRevision::new(1).unwrap(),
            ConversationParent::Root,
            Some(owner),
            None,
            empty_composer_content(),
            timestamp(5),
            timestamp(5),
        ))]),
    );
}
