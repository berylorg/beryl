use super::*;
use crate::syndic_transcript::{
    CONTEXT_CHUNK_MAX_BYTES, ResidentPresentationRecordKind, ResidentRecordSource,
    ResidentTranscriptCore, TranscriptActivationSource, TranscriptProviderResponseKind,
};
use beryl_model::{DiscussionContextOwnerId, SyndicPathDigest, SyndicTurnId, ThreadRevision};
use syndic_storage::*;

fn source_turn() -> SyndicTurnId {
    SyndicTurnId::from_bytes([12; 16])
}

pub(super) fn seed_context(fixture: &Fixture, text: Option<&str>, submitted: bool) {
    let point = SyndicPointReadLimit::new(66_048).unwrap();
    let id = fixture.request(1).thread_id;
    let old = fixture
        .storage
        .thread(&fixture.home, id, point)
        .unwrap()
        .unwrap();
    let draft = fixture
        .storage
        .draft(&fixture.home, old.current_draft_id(), point)
        .unwrap()
        .unwrap();
    let owner = if submitted {
        DiscussionContextOwnerId::submitted_from_draft(draft.id())
    } else {
        DiscussionContextOwnerId::Draft(draft.id())
    };
    let parent = SyndicThreadId::from_bytes([2; 16]);
    let digest = SyndicPathDigest::from_bytes([7; 32]);
    let selected =
        SelectedPathProof::new(Some(source_turn()), ThreadRevision::new(2).unwrap(), digest);
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::Thread(ThreadRecord::new(
            id,
            selected,
            old.current_draft_id(),
            ThreadLineageProof::new(
                Some(parent),
                None,
                ThreadLineageDepth::new(2).unwrap(),
                digest,
            ),
            Some(owner),
        )))
        .unwrap();
    batch
        .put(FixtureRecord::Draft(DraftRecord::new(
            draft.id(),
            id,
            draft.revision(),
            DraftSubmissionIntent::DiscussionContext(DiscussionContextOwnerId::Draft(draft.id())),
            draft.root_history(),
            draft.created_at(),
            draft.updated_at(),
        )))
        .unwrap();
    batch
        .put(FixtureRecord::Turn(TurnRecord::new(
            source_turn(),
            parent,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::new(1).unwrap(),
            digest,
            SyndicTimestamp::from_unix_millis(1),
        )))
        .unwrap();
    if let DiscussionContextOwnerId::SubmittedTurn(turn) = owner {
        batch
            .put(FixtureRecord::Turn(TurnRecord::new(
                turn,
                id,
                TurnKind::OrdinaryUser,
                ConversationParent::Turn(source_turn()),
                None,
                TurnDepth::new(2).unwrap(),
                digest,
                SyndicTimestamp::from_unix_millis(2),
            )))
            .unwrap();
    }
    if let Some(text) = text {
        batch
            .put(FixtureRecord::ContextEnvelope(ContextEnvelopeRecord::new(
                owner,
                ContextEnvelopeRevision::new(1).unwrap(),
                DiscussionContextEnvelope::new(
                    DiscussionContextSource::new(
                        parent,
                        source_turn(),
                        beryl_model::SyndicItemId::from_bytes([11; 16]),
                        beryl_model::SyndicProjectionId::from_bytes([13; 16]),
                        ProjectionRevision::new(1).unwrap(),
                        DiscussionContextRange::new(0, text.len() as u64).unwrap(),
                    ),
                    DiscussionContextText::new(text).unwrap(),
                    SyndicTimestamp::from_unix_millis(1),
                )
                .unwrap(),
            )))
            .unwrap();
    }
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
    );
    fixture.seed_head(1);
    fixture.seed_projection("source turn contents", 1);
}

fn activation(fixture: &Fixture) -> PreparedTranscriptActivation {
    fixture
        .reader
        .read_activation(&fixture.request(1), &AtomicBool::new(false))
        .unwrap()
        .2
}

#[test]
fn context_exact_source_bounds_multibyte_copy_and_non_authored_provenance() {
    for text in ["# Context\n**selected**\n".to_owned(), "🍒".repeat(16_384)] {
        let fixture = Fixture::new();
        seed_context(&fixture, Some(&text), false);
        let prepared = activation(&fixture);
        let group = prepared.discussion_context.clone().unwrap();
        assert_eq!(group.provenance.insertion_after, 1);
        assert_eq!(
            group
                .chunks
                .iter()
                .map(|chunk| chunk.text.as_ref())
                .collect::<String>(),
            text
        );
        assert!(group.chunks.len() <= 33);
        assert!(
            group
                .chunks
                .iter()
                .all(|chunk| chunk.text.len() <= CONTEXT_CHUNK_MAX_BYTES)
        );
        let mut core = ResidentTranscriptCore::empty();
        core.apply_prepared_activation(prepared, TranscriptActivationSource::RunningThread);
        let snapshot = core.presentation_snapshot();
        assert_eq!(snapshot.records.len(), 1 + group.chunks.len());
        for record in snapshot.records.iter().skip(1) {
            assert!(matches!(
                record.provenance.source,
                ResidentRecordSource::LocalContext { .. }
            ));
            assert!(record.provenance.projection_id.is_none());
            assert!(record.provenance.projection_revision.is_none());
            let selected = crate::syndic_transcript::resident_selected_record(record);
            assert!(selected.is_ok());
        }
        assert_eq!(core.core_snapshot().resident.projection_record_count, 1);
        assert!(core.core_snapshot().resident.presentation_bytes >= text.len() * 2);
    }
}

#[test]
fn context_identity_survives_submission_and_permanent_owner_off_selected_path() {
    let fixture = Fixture::new();
    seed_context(&fixture, Some("**immutable**"), false);
    let first = activation(&fixture).discussion_context.unwrap();
    seed_context(&fixture, Some("**immutable**"), true);
    let submitted = activation(&fixture).discussion_context.unwrap();
    assert_eq!(first.provenance.group_id, submitted.provenance.group_id);
    assert_eq!(first.chunks, submitted.chunks);
    assert_eq!(
        first.provenance.insertion_parent,
        submitted.provenance.insertion_parent
    );
}

#[test]
fn missing_envelope_keeps_unavailable_group_without_copy_or_substitute_text() {
    let fixture = Fixture::new();
    seed_context(&fixture, None, false);
    let prepared = activation(&fixture);
    let group = prepared.discussion_context.unwrap();
    assert!(group.chunks.is_empty());
    let record = group.presentation_records(1).remove(0);
    assert!(matches!(
        record.kind,
        ResidentPresentationRecordKind::ContextUnavailable
    ));
    assert!(crate::syndic_transcript::resident_selected_record(&record).is_err());
    assert_eq!(group.provenance.insertion_after, 1);
}

#[test]
fn context_boundary_follows_complete_source_turn_across_page_split() {
    let fixture = Fixture::new();
    seed_context(&fixture, Some("**immutable selection**"), false);
    let thread = fixture
        .storage
        .thread(
            &fixture.home,
            fixture.request(1).thread_id,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut batch = FixtureBatch::new();
    for position in 2..=70 {
        let projection_id = beryl_model::SyndicProjectionId::from_bytes([position as u8; 16]);
        batch
            .put(FixtureRecord::Projection(
                syndic_storage::ProjectionRecord::new(
                    projection_id,
                    ProjectionRevision::new(1).unwrap(),
                    beryl_model::SyndicItemId::from_bytes([11; 16]),
                    source_turn(),
                    ProjectionOrdinal::new(position).unwrap(),
                    syndic_storage::ProjectionPayload::inline_markdown(
                        MarkdownBlockId::from_bytes([position as u8; 32]),
                        MarkdownBlockKind::Paragraph,
                        position,
                        ProjectionSourceRange::new(0, 4).unwrap(),
                        "body",
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        batch
            .put(FixtureRecord::TranscriptViewEntry(
                TranscriptViewEntryRecord::new(
                    thread.id(),
                    TranscriptGeneration::new(1).unwrap(),
                    TranscriptPosition::new(position).unwrap(),
                    beryl_model::SyndicItemId::from_bytes([11; 16]),
                    ProjectionRevision::new(1).unwrap(),
                    ItemProjectionGeneration::new(1).unwrap(),
                    projection_id,
                    ProjectionRevision::new(1).unwrap(),
                ),
            ))
            .unwrap();
    }
    batch
        .put(FixtureRecord::TranscriptViewHead(
            TranscriptViewHeadRecord::new(
                thread.id(),
                TranscriptGeneration::new(1).unwrap(),
                ProjectionRevision::new(1).unwrap(),
                70,
                thread.committed_tail(),
                thread.selected_path_digest(),
                ProjectionLifecycle::Current,
            ),
        ))
        .unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
    );
    let mut request = fixture.request(1);
    request.placement = TranscriptActivationPlacement::Start;
    let start = fixture
        .reader
        .read_activation(&request, &AtomicBool::new(false))
        .unwrap()
        .2;
    assert_eq!(
        start
            .discussion_context
            .as_ref()
            .unwrap()
            .provenance
            .insertion_after,
        70
    );
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(start, TranscriptActivationSource::RunningThread);
    assert!(
        core.presentation_snapshot()
            .records
            .iter()
            .all(|record| !matches!(
                record.provenance.source,
                ResidentRecordSource::LocalContext { .. }
            ))
    );
    core.apply_prepared_activation(
        activation(&fixture),
        TranscriptActivationSource::RunningThread,
    );
    let snapshot = core.presentation_snapshot();
    assert!(matches!(
        snapshot.records.last().unwrap().provenance.source,
        ResidentRecordSource::LocalContext { .. }
    ));
}

#[test]
fn invalid_envelope_agreement_keeps_boundary_and_missing_owner_refuses_source() {
    let fixture = Fixture::new();
    seed_context(&fixture, Some("exact"), true);
    let point = SyndicPointReadLimit::new(66_048).unwrap();
    let thread = fixture
        .storage
        .thread(&fixture.home, fixture.request(1).thread_id, point)
        .unwrap()
        .unwrap();
    let owner = thread.context_owner_id().unwrap();
    let invalid = DiscussionContextEnvelope::new(
        DiscussionContextSource::new(
            SyndicThreadId::from_bytes([88; 16]),
            source_turn(),
            beryl_model::SyndicItemId::from_bytes([11; 16]),
            beryl_model::SyndicProjectionId::from_bytes([13; 16]),
            ProjectionRevision::new(1).unwrap(),
            DiscussionContextRange::new(0, 5).unwrap(),
        ),
        DiscussionContextText::new("exact").unwrap(),
        SyndicTimestamp::from_unix_millis(1),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::ContextEnvelope(ContextEnvelopeRecord::new(
            owner,
            ContextEnvelopeRevision::new(2).unwrap(),
            invalid,
        )))
        .unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
    );
    let group = activation(&fixture).discussion_context.unwrap();
    assert!(group.chunks.is_empty());
    assert_eq!(group.provenance.insertion_after, 1);
    let DiscussionContextOwnerId::SubmittedTurn(owner) = owner else {
        unreachable!()
    };
    let mut batch = FixtureBatch::new();
    batch
        .delete(syndic_storage::test_faults::FixtureDelete::Turn(owner))
        .unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
    );
    assert!(matches!(
        fixture
            .reader
            .read_activation(&fixture.request(1), &AtomicBool::new(false)),
        Err(TranscriptAttachmentError::Unavailable)
    ));
}

#[test]
fn authored_projection_capacity_preserves_exact_context_and_bounded_boundary_fallback() {
    let fixture = Fixture::new();
    let text = "🍒".repeat(16_384);
    seed_context(&fixture, Some(&text), false);
    fixture.seed_projection(&"x".repeat(65_536), 1);
    let seed = activation(&fixture);
    assert_eq!(
        seed.discussion_context
            .as_ref()
            .unwrap()
            .chunks
            .iter()
            .map(|chunk| chunk.text.len())
            .sum::<usize>(),
        65_536
    );
    let Some(TranscriptProviderResponseKind::ViewPage(page)) = &seed.view_page_response else {
        unreachable!()
    };
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].position.0, 1);
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
    let snapshot = core.presentation_snapshot();
    assert!(matches!(
        snapshot.records[0].provenance.source,
        ResidentRecordSource::LocalUiForSyndic(_)
    ));
    assert_eq!(snapshot.record_count(), 33);
    assert_eq!(core.core_snapshot().resident.projection_record_count, 0);
    assert_eq!(core.core_snapshot().resident.view_record_count, 1);
}
