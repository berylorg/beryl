use beryl_model::{BindingRevision, DiscussionContextOwnerId, DraftRevision, ProjectionRevision};
use syndic_storage::test_faults::{FixtureRecord, fixture_transcript_digest_seed};
use syndic_storage::*;

use super::{source_item, source_projection, source_turn};
use crate::support::{canonical_empty_root_history_pair_for, draft_id, id, timestamp};

pub(super) fn records() -> Vec<FixtureRecord> {
    let source_thread = id(30);
    let child_thread = id(36);
    let child_draft = draft_id(37);
    let source_turn = source_turn();
    let owner = DiscussionContextOwnerId::Draft(child_draft);
    let revision = ProjectionRevision::new(1).unwrap();
    let thread_revision = beryl_model::ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let source = DiscussionContextSource::new(
        source_thread,
        source_turn,
        source_item(),
        source_projection(),
        revision,
        DiscussionContextRange::new(0, 9).unwrap(),
    );
    let context = DiscussionContextEnvelope::new(
        source,
        DiscussionContextText::new("assistant").unwrap(),
        timestamp(5),
    )
    .unwrap();
    let empty_digest = empty_selected_path_digest();
    let selected = SelectedPathProof::new(None, thread_revision, empty_digest);
    let lineage = ThreadLineageProof::new(
        Some(source_thread),
        Some(source_thread),
        ThreadLineageDepth::new(2).unwrap(),
        child_thread_lineage_digest(
            child_thread,
            source_thread,
            root_thread_lineage_digest(source_thread),
        ),
    );
    let thread = ThreadRecord::new(child_thread, selected, child_draft, lineage, Some(owner));
    let execution = ThreadExecutionRecord::new(child_thread, super::source::execution_binding());
    let attributes = syndic_storage::test_faults::open_branch_thread_attributes(child_thread);
    let usage = ThreadUsageRecord::empty(child_thread);
    let history = HistorySummaryRecord::new(
        child_thread,
        revision,
        thread_revision,
        None,
        empty_digest,
        true,
        timestamp(5),
    );
    let catalog = ThreadCatalogSummaryRecord::initial(&thread, &execution, &attributes, &history);

    let records = vec![
        FixtureRecord::Thread(thread),
        FixtureRecord::ImageLabelAuthorityHead(
            ImageLabelAuthorityHeadV1::new(
                child_thread,
                1,
                ImageLabelFrontier::EMPTY,
                ImageLabelFrontier::EMPTY,
            )
            .unwrap(),
        ),
        FixtureRecord::ThreadExecution(execution),
        FixtureRecord::ThreadAttributes(attributes),
        FixtureRecord::ThreadUsage(usage),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::Draft(DraftRecord::new(
            child_draft,
            child_thread,
            draft_revision,
            DraftSubmissionIntent::DiscussionContext(owner),
            canonical_empty_root_history_pair_for(child_draft),
            timestamp(5),
            timestamp(5),
        )),
        FixtureRecord::ActivityQueryHead(ActivityQueryHeadRecord::empty(child_thread)),
        FixtureRecord::ContextEnvelope(ContextEnvelopeRecord::new(
            owner,
            ContextEnvelopeRevision::FIRST,
            context,
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            child_thread,
            child_draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::InputGate(InputGateRecord::idle(child_thread)),
        FixtureRecord::ThreadParent(ThreadParentIndexRecord::new(
            source_thread,
            child_thread,
            thread_revision,
            owner,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            child_thread,
            TranscriptGeneration::FIRST,
            revision,
            0,
            None,
            empty_digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            child_thread,
            TranscriptGeneration::FIRST,
            revision,
            thread_revision,
            None,
            empty_digest,
            0,
            0,
            fixture_transcript_digest_seed(),
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::Binding(BindingRecord::new(
            child_thread,
            binding_revision,
            selected,
            BindingState::unbound("child fixture").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            child_thread,
            binding_revision,
            BindingLifecycle::Unbound,
            empty_digest,
        )),
    ];
    records
}
