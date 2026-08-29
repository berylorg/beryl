use beryl_model::{
    AcceptedInputRevision, BindingRevision, ContentRevision, DraftRevision, ExecutionBinding,
    InputGateRevision, ProjectionRevision, SyndicDraftId, SyndicThreadId, SyndicTurnId,
    ThreadRevision,
};
use syndic_storage::test_faults::{FixtureRecord, fixture_transcript_digest_seed};
use syndic_storage::*;

use super::time;

pub(super) fn prepared_content_records(
    content: &PreparedContent,
) -> (ContentReference, Vec<FixtureRecord>) {
    let revision = ContentRevision::new(1).unwrap();
    let mut records = vec![FixtureRecord::ContentManifest(
        content.sealed_manifest(revision),
    )];
    let mut encoded_start = 0;
    for chunk in content.chunks() {
        records.push(FixtureRecord::ContentChunk(chunk.clone()));
        let span = ContentByteSpanRecord::for_chunk(chunk, encoded_start).unwrap();
        encoded_start = span.end();
        records.push(FixtureRecord::ContentByteSpan(span));
    }
    records.extend(
        content
            .text_spans()
            .iter()
            .copied()
            .map(FixtureRecord::ContentTextSpan),
    );
    records.extend(
        content
            .pieces()
            .iter()
            .copied()
            .map(FixtureRecord::ContentPiece),
    );
    (content.reference(revision), records)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn promotion_records(
    thread: SyndicThreadId,
    current_draft: SyndicDraftId,
    source_draft: SyndicDraftId,
    parent: SyndicTurnId,
    execution: ExecutionBinding,
    accepted_content: ContentReference,
    draft_content: ContentReference,
    accepted_proof: Option<beryl_model::SealedAssetReferenceSetProof>,
    image_bearing: bool,
) -> Vec<FixtureRecord> {
    let thread_revision = ThreadRevision::new(2).unwrap();
    let draft_revision = DraftRevision::new(2).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let parent_digest = root_turn_chain_digest(parent);
    let selected = SelectedPathProof::new(Some(parent), thread_revision, parent_digest);
    let image_label_authority = ImageLabelAuthorityHeadV1::new(
        thread,
        2,
        ImageLabelFrontier::EMPTY,
        if image_bearing {
            ImageLabelFrontier::from_raw(1)
        } else {
            ImageLabelFrontier::EMPTY
        },
    )
    .unwrap();
    let accepted_input = source_draft.accepted_input_id();
    let accepted_ordinal = AcceptedInputOrdinal::FIRST;
    let route_generation = AcceptedRouteGeneration::FIRST;
    let accepted_bytes = accepted_content.summary().logical_utf8_bytes();
    let thread_record = ThreadRecord::new(
        thread,
        selected,
        current_draft,
        ThreadLineageProof::new(
            None,
            None,
            ThreadLineageDepth::FIRST,
            root_thread_lineage_digest(thread),
        ),
        None,
    );
    let history_summary = HistorySummaryRecord::new(
        thread,
        ProjectionRevision::new(1).unwrap(),
        thread_revision,
        Some(parent),
        parent_digest,
        true,
        time(15),
    );
    let thread_execution = ThreadExecutionRecord::new(thread, execution);
    let thread_attributes = ThreadAttributesRecord::ordinary(thread);
    let thread_catalog = ThreadCatalogSummaryRecord::initial(
        &thread_record,
        &thread_execution,
        &thread_attributes,
        &history_summary,
    );
    let mut records = vec![
        FixtureRecord::Thread(thread_record),
        FixtureRecord::ImageLabelAuthorityHead(image_label_authority),
        FixtureRecord::ThreadExecution(thread_execution),
        FixtureRecord::ThreadAttributes(thread_attributes),
        FixtureRecord::ThreadUsage(ThreadUsageRecord::empty(thread)),
        FixtureRecord::ThreadCatalogSummary(thread_catalog),
        FixtureRecord::Draft(DraftRecord::new(
            current_draft,
            thread,
            draft_revision,
            DraftSubmissionIntent::Ordinary,
            draft_content,
            time(1),
            time(15),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            current_draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(2).unwrap(),
                InputGateState::Idle,
                1,
                Some(route_generation),
                None,
                0,
                1,
                accepted_bytes,
            )
            .unwrap(),
        ),
        FixtureRecord::ActivityQueryHead(ActivityQueryHeadRecord::empty(thread)),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            0,
            Some(parent),
            parent_digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::HistorySummary(history_summary),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            SelectedPathProof::new(Some(parent), ThreadRevision::new(1).unwrap(), parent_digest),
            BindingState::unbound("phase 58 app promotion fixture").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Unbound,
            parent_digest,
        )),
        FixtureRecord::Turn(TurnRecord::new(
            parent,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            parent_digest,
            time(1),
        )),
        FixtureRecord::TurnState(
            TurnStateRecord::new(
                parent,
                TurnStateRevision::FIRST,
                TurnLifecycle::Failed,
                1,
                0,
                Some(TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap()),
                time(5),
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                parent,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
    ];
    records.extend([
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            thread_revision,
            Some(parent),
            parent_digest,
            1,
            0,
            fixture_transcript_digest_seed(),
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TurnDepth::FIRST,
            parent,
            parent_digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Failed,
            1,
            0,
            0,
            time(5),
        )),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                accepted_input,
                thread,
                accepted_ordinal,
                AcceptedInputAdmissionProof::new(
                    ThreadRevision::new(1).unwrap(),
                    source_draft,
                    DraftRevision::new(1).unwrap(),
                    InputGateRevision::new(1).unwrap(),
                    current_draft,
                )
                .unwrap(),
                route_generation,
                accepted_content,
                accepted_proof,
                time(1),
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread,
            accepted_ordinal,
            accepted_input,
            route_generation,
        )),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                route_generation,
                AcceptedRouteRevision::FIRST,
                AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
                Some(accepted_ordinal),
                Some(accepted_ordinal),
                1,
                0,
                0,
                1,
                0,
                accepted_bytes,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
            thread,
            route_generation,
            AcceptedRouteRevision::FIRST,
            accepted_ordinal,
            accepted_ordinal,
        )),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            accepted_input,
            thread,
            route_generation,
            accepted_ordinal,
            AcceptedInputRevision::new(1).unwrap(),
            AcceptedRouteLeafState::NextTurn(NextTurnReason::PendingTurn),
            AcceptedInputLifecycle::Admitted,
        )),
    ]);
    if let Some(proof) = accepted_proof {
        records.push(FixtureRecord::ImageLabelOriginSpan(
            ImageLabelOriginSpanRecord::new(
                thread,
                ImageLabelOrdinal::FIRST,
                ImageLabelOrdinal::FIRST,
                ImageLabelOriginOwner::AcceptedInput(accepted_input),
                proof,
            )
            .unwrap(),
        ));
    }
    records
}
