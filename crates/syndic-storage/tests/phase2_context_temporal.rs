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
    fixture_turn_state, fixture_turn_state_with_finalization, id, open, timestamp,
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
        ProjectionRevision::new(4).unwrap(),
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
    let item_revision = ProjectionRevision::new(4).unwrap();
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
        ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::new(2).unwrap()),
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
        item_revision,
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
            None,
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
            item_revision,
            ProjectionTextSource::composer(content),
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
            item_revision,
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
    correlate_source_user_item(&mut records, item, revision, content, None, timestamp(4));
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

fn unknown_terminal_source_mutation() -> FixtureBatch {
    let records = populated_records();
    let item = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::CanonicalItem(item) if item.id() == source_item() => Some(item.clone()),
            _ => None,
        })
        .unwrap();
    let frame = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::SourceEvent(event)
                if event.turn_id() == source_turn()
                    && event.sequence() == SourceEventSequence::new(4).unwrap() =>
            {
                match event.payload() {
                    SourceEventPayload::ItemFrame { frame, .. } => Some(frame.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
        .unwrap();
    let set = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::ItemProjectionSet(set) if set.item_id() == source_item() => {
                Some(set.clone())
            }
            _ => None,
        })
        .unwrap();
    let head = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::ItemProjectionHead(head) if head.item_id() == source_item() => {
                Some(*head)
            }
            _ => None,
        })
        .unwrap();
    let live_revision = ProjectionRevision::new(3).unwrap();
    let (provider, manifest) = syndic_storage::test_faults::fixture_provider_content_manifest(
        source_item(),
        &frame,
        false,
    );
    let cas_source = item.cas_source().unwrap().clone();
    let canonical = CanonicalItemRecord::with_provider_state(
        item.id(),
        item.turn_id(),
        item.ordinal(),
        live_revision,
        SourceEventSequence::new(4).unwrap(),
        3,
        cas_source.clone(),
        item.assistant_phase(),
        provider,
        item.narrative_completion(),
        item.presentation().clone(),
    )
    .unwrap();
    let updated_set = ItemProjectionSetRecord::new(
        set.item_id(),
        set.generation(),
        set.format(),
        live_revision,
        set.source(),
        set.source_bytes(),
        set.stable_projection_count(),
        set.stable_resource_count(),
        set.stable_digest(),
        set.projection_count(),
        set.resource_count(),
        set.digest(),
        set.resume_checkpoint().clone(),
        set.stable_eof_resolved(),
    );
    let mut mutation = batch([
        FixtureRecord::InputGate(
            InputGateRecord::new(
                id(30),
                InputGateRevision::new(1).unwrap(),
                InputGateState::PendingTurn(source_turn()),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::TurnState(fixture_turn_state_with_finalization(
            source_turn(),
            TurnStateRevision::FIRST,
            TurnLifecycle::UnknownTerminal,
            5,
            1,
            0,
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
        FixtureRecord::ContentManifest(manifest),
        FixtureRecord::CanonicalItem(canonical),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            source_turn(),
            TurnItemOrdinal::FIRST,
            source_item(),
            live_revision,
        )),
        FixtureRecord::CasItem(CasItemIndexRecord::new(
            cas_source.turn().thread_id().clone(),
            cas_source.turn().turn_id().clone(),
            cas_source.item_id().clone(),
            source_item(),
            live_revision,
        )),
        FixtureRecord::ItemProjectionSet(updated_set),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            source_item(),
            head.revision(),
            live_revision,
            head.generation(),
            head.lifecycle(),
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            id(30),
            TranscriptGeneration::FIRST,
            ProjectionRevision::new(1).unwrap(),
            0,
            Some(source_turn()),
            source_digest(),
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            id(30),
            TranscriptGeneration::FIRST,
            ProjectionRevision::new(1).unwrap(),
            ThreadRevision::new(1).unwrap(),
            Some(source_turn()),
            source_digest(),
            2,
            0,
            fixture_transcript_digest_seed(),
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
            0,
            timestamp(4),
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            id(30),
            ProjectionRevision::new(2).unwrap(),
            ThreadRevision::new(1).unwrap(),
            Some(source_turn()),
            source_digest(),
            false,
            timestamp(4),
        )),
    ]);
    mutation
        .delete(FixtureDelete::TranscriptViewEntry {
            thread: id(30),
            generation: TranscriptGeneration::FIRST,
            position: TranscriptPosition::FIRST,
        })
        .unwrap();
    mutation
}

#[path = "phase2_context_temporal/child_path.rs"]
mod child_path;
#[path = "phase2_context_temporal/reopen_validation.rs"]
mod reopen_validation;
#[path = "phase2_context_temporal/selected_away.rs"]
mod selected_away;
