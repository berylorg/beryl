#![cfg(feature = "test-faults")]

mod support;

#[path = "phase5_replacement_root/native_turn_count.rs"]
mod native_turn_count;

use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{
    BindingRevision, DraftRevision, ExecutionBinding, InputGateRevision, PathFlavor,
    ProjectionRevision, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicItemId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureRecord, fixture_advance_item_projection_digest, fixture_advance_transcript_digest,
    fixture_inline_paragraph_projection, fixture_item_projection_digest_seed,
    fixture_transcript_digest_seed,
};
use syndic_storage::*;

use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean replacement-root fixture command, got {outcome:?}"),
    }
}

fn thread() -> SyndicThreadId {
    id(80)
}

fn draft() -> SyndicDraftId {
    draft_id(81)
}

fn root_turn() -> SyndicTurnId {
    SyndicTurnId::from_bytes([82; 16])
}

fn root_item() -> SyndicItemId {
    SyndicItemId::from_bytes([83; 16])
}

fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([87; 16]),
        RootId::from_bytes([88; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\phase10-root-fresh",
        )
        .unwrap(),
    )
}

fn root_fixture() -> syndic_storage::test_faults::FixtureBatch {
    let thread = thread();
    let draft = draft();
    let turn = root_turn();
    let item = root_item();
    let thread_revision = ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let digest = root_turn_chain_digest(turn);
    let payload = ComposerPayload::new(vec![ComposerAtom::text("root original").unwrap()]).unwrap();
    let (content, content_records) = composer_content_records(&payload);
    let projection_record = fixture_inline_paragraph_projection(item, turn, "root original");
    let projection = projection_record.id();
    let projection_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection,
        projection_revision,
    );
    let transcript_entry = TranscriptViewEntryRecord::new(
        thread,
        TranscriptGeneration::FIRST,
        TranscriptPosition::FIRST,
        item,
        projection_revision,
        ItemProjectionGeneration::FIRST,
        projection,
        projection_revision,
    );
    let transcript_digest =
        fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &transcript_entry);
    let (empty, empty_records) = composer_content_records(&ComposerPayload::default());
    let thread_record = ThreadRecord::new(
        thread,
        SelectedPathProof::new(Some(turn), thread_revision, digest),
        draft,
        ThreadLineageProof::new(
            None,
            None,
            syndic_storage::ThreadLineageDepth::FIRST,
            syndic_storage::root_thread_lineage_digest(thread),
        ),
        syndic_storage::ThreadImageLabelFrontiers::empty(),
        None,
    );
    let execution = ThreadExecutionRecord::new(thread, execution_binding());
    let attributes = ThreadAttributesRecord::ordinary(thread);
    let usage = ThreadUsageRecord::empty(thread);
    let history = HistorySummaryRecord::new(
        thread,
        projection_revision,
        thread_revision,
        Some(turn),
        digest,
        false,
        timestamp(2),
    );
    let catalog =
        ThreadCatalogSummaryRecord::initial(&thread_record, &execution, &attributes, &history);

    let mut records = vec![
        FixtureRecord::Thread(thread_record),
        FixtureRecord::ThreadExecution(execution),
        FixtureRecord::ThreadAttributes(attributes),
        FixtureRecord::ThreadUsage(usage),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::Draft(DraftRecord::new(
            draft,
            thread,
            draft_revision,
            DraftSubmissionIntent::Ordinary,
            empty,
            timestamp(2),
            timestamp(2),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::InputGate(InputGateRecord::idle(thread)),
        FixtureRecord::ActivityQueryHead(ActivityQueryHeadRecord::empty(thread)),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(1),
        )),
        FixtureRecord::TurnState(
            TurnStateRecord::with_capture_frontiers(
                turn,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                1,
                1,
                1,
                1,
                0,
                Some(
                    TurnEndStatus::new(
                        TurnTerminalOutcome::Interrupted,
                        Some(TurnIncompleteReason::ItemAuditFailed),
                    )
                    .unwrap(),
                ),
                timestamp(2),
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(
                        TurnTerminalOutcome::Interrupted,
                        Some(TurnIncompleteReason::ItemAuditFailed),
                    )
                    .unwrap(),
                ),
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            projection_revision,
            content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            projection_revision,
        )),
    ];
    records.extend(content_records);
    records.extend(empty_records);
    records.extend([
        FixtureRecord::Projection(projection_record),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection,
            projection_revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            projection_revision,
            ProjectionTextSource::composer(content),
            13,
            1,
            0,
            projection_digest,
            1,
            0,
            projection_digest,
            MarkdownParserCheckpoint::new(
                13,
                13,
                ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::new(2).unwrap()),
                13,
                Box::<str>::default(),
                false,
                None,
            ),
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            projection_revision,
            projection_revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            1,
            Some(turn),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            thread_revision,
            Some(turn),
            digest,
            1,
            1,
            transcript_digest,
            false,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TurnDepth::FIRST,
            turn,
            digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            1,
            1,
            timestamp(2),
        )),
        FixtureRecord::TranscriptViewEntry(transcript_entry),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            SelectedPathProof::new(Some(turn), thread_revision, digest),
            BindingState::unbound("root replacement fixture").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        )),
    ]);
    batch(records)
}

#[test]
fn accepted_replacement_of_a_root_turn_creates_another_root() {
    let home = TestHome::new("phase5-replacement-root");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, root_fixture());
    let selected = SelectedPathProof::new(
        Some(root_turn()),
        ThreadRevision::new(1).unwrap(),
        root_turn_chain_digest(root_turn()),
    );
    execute(
        &store,
        storage.start_replacement_edit(
            storage.revision(&store).unwrap(),
            StartReplacementEdit::new(
                thread(),
                ThreadRevision::new(1).unwrap(),
                draft(),
                DraftRevision::new(1).unwrap(),
                InputGateRevision::new(1).unwrap(),
                root_turn(),
                root_item(),
                selected,
                CurrentTranscriptEntryProof::new(
                    TranscriptGeneration::FIRST,
                    TranscriptPosition::FIRST,
                ),
                None,
                timestamp(3),
            ),
        ),
    );

    let editing = storage
        .current_draft(&store, thread(), point_limit())
        .unwrap()
        .unwrap();
    let replacement_turn = draft().submitted_turn_id();
    let next_draft = draft_id(85);
    execute(
        &store,
        storage.submit_idle_draft(
            storage.revision(&store).unwrap(),
            IdleSubmission::new(
                thread(),
                ThreadRevision::new(1).unwrap(),
                draft(),
                DraftRevision::new(2).unwrap(),
                editing.draft().content(),
                InputGateRevision::new(1).unwrap(),
                next_draft,
                SyndicItemId::from_bytes([86; 16]),
                None,
                timestamp(4),
            ),
        ),
    );
    store.validate_registered_domains().unwrap();

    let replacement = storage
        .turn(&store, replacement_turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(replacement.parent(), ConversationParent::Root);
    let original = storage
        .turn(&store, root_turn(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(original.parent(), ConversationParent::Root);
    let current = storage
        .current_draft(&store, thread(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.thread().committed_tail(), Some(replacement_turn));
    assert_eq!(current.draft().id(), next_draft);
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(
        storage
            .input_gate(&store, thread(), point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::PendingTurn(replacement_turn),
    );
    store.close().unwrap();
}
