#![cfg(feature = "test-faults")]

mod support;

use beryl_model::{SyndicDraftId, SyndicThreadId, SyndicTurnId, ThreadRevision};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_advance_transcript_digest, fixture_inline_paragraph_projection,
    fixture_item_projection_digest_seed, fixture_transcript_digest_seed,
};
use syndic_storage::*;

use support::populated::{source_item, source_turn};
use support::semantic::exercise_seeded_populated_case;
use support::*;

fn replacement_mutation(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    target: SyndicTurnId,
    selected: SelectedPathProof,
    entry: CurrentTranscriptEntryProof,
) -> FixtureBatch {
    let current = storage
        .current_draft(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let summary = storage
        .history_summary(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let revision = current.draft().revision().checked_next().unwrap();
    batch([
        FixtureRecord::Draft(DraftRecord::new(
            current.draft().id(),
            current.thread().id(),
            revision,
            DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(target, selected, entry)),
            current.draft().root_history(),
            current.draft().created_at(),
            summary.last_activity_at(),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            current.thread().id(),
            current.draft().id(),
            revision,
            current.thread().revision(),
        )),
    ])
}

fn seed_empty_replacement_thread(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
) {
    let mut command = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                support::exact_cas::execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean empty replacement seed, got {outcome:?}"),
    }
}

fn seed_real_replacement_target(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
) -> (SyndicTurnId, SelectedPathProof, CurrentTranscriptEntryProof) {
    let turn = source_turn();
    let item = source_item();
    let current = storage
        .current_draft(store, id(30), SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, id(30), SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            store,
            id(30),
            head.generation(),
            None,
            beryl_home_store::CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == item)
        .unwrap();
    (
        turn,
        current.thread().selected_path(),
        CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
    )
}

fn seed_local_user_replacement_target(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
) -> (SyndicTurnId, SelectedPathProof, CurrentTranscriptEntryProof) {
    seed_empty_replacement_thread(store, storage, thread, draft);
    let turn = SyndicTurnId::from_bytes([92; 16]);
    let item = beryl_model::SyndicItemId::from_bytes([93; 16]);
    let digest = root_turn_chain_digest(turn);
    let projection = fixture_inline_paragraph_projection(item, turn, "replacement");
    let revision = projection.revision();
    let (content, mut records) = composer_content_records(
        &ComposerPayload::new(vec![ComposerAtom::text("replacement").unwrap()]).unwrap(),
    );
    let item_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection.id(),
        revision,
    );
    let entry = TranscriptViewEntryRecord::new(
        thread,
        TranscriptGeneration::FIRST,
        TranscriptPosition::FIRST,
        item,
        revision,
        ItemProjectionGeneration::FIRST,
        projection.id(),
        revision,
    );
    let transcript_digest =
        fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &entry);
    let thread_revision = ThreadRevision::new(1).unwrap();
    let selected = SelectedPathProof::new(Some(turn), thread_revision, digest);
    let thread_record = ThreadRecord::new(
        thread,
        selected,
        draft,
        ThreadLineageProof::new(
            None,
            None,
            ThreadLineageDepth::FIRST,
            root_thread_lineage_digest(thread),
        ),
        ThreadImageLabelFrontiers::empty(),
        None,
    );
    let execution = ThreadExecutionRecord::new(thread, support::exact_cas::execution_binding());
    let attributes = ThreadAttributesRecord::ordinary(thread);
    let history = HistorySummaryRecord::new(
        thread,
        revision,
        thread_revision,
        Some(turn),
        digest,
        false,
        timestamp(2),
    );
    let catalog = ThreadCatalogSummaryRecord::new(
        thread,
        revision,
        Some(
            ThreadCatalogTitle::new("replacement", ThreadCatalogTitleSource::HistoryDerived)
                .unwrap(),
        ),
        execution.execution().clone(),
        attributes.archive(),
        history.last_activity_at(),
        history.complete(),
        thread_record.parent_thread_id(),
        thread_record.lineage_depth(),
        thread_record.lineage_digest(),
        ThreadCatalogSourceWitnesses::new(
            attributes.revision(),
            history.revision(),
            history.thread_revision(),
            history.selected_path_digest(),
            thread_record.revision(),
        ),
    );
    let mut thread_fixture = thread_records(thread, draft, Some(turn), digest);
    thread_fixture.retain(|record| {
        !matches!(
            record,
            FixtureRecord::TranscriptViewHead(_)
                | FixtureRecord::HistorySummary(_)
                | FixtureRecord::ThreadCatalogSummary(_)
        )
    });
    records.extend(thread_fixture);
    records.extend([
        FixtureRecord::HistorySummary(history),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(2),
        )),
        FixtureRecord::TurnState(fixture_turn_state_with_capture(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Incomplete,
            1,
            1,
            1,
            1,
            0,
            timestamp(2),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
                    TurnIncompleteReason::ItemAuditFailed,
                )),
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            revision,
        )),
        FixtureRecord::Projection(projection.clone()),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection.id(),
            revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            revision,
            ProjectionTextSource::composer(content),
            11,
            1,
            0,
            item_digest,
            1,
            0,
            item_digest,
            MarkdownParserCheckpoint::new(
                11,
                11,
                ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::new(2).unwrap()),
                11,
                Box::<str>::default(),
                false,
                None,
            ),
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            revision,
            revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptViewEntry(entry),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TurnDepth::FIRST,
            turn,
            digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Incomplete,
            1,
            1,
            1,
            timestamp(2),
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            revision,
            thread_revision,
            Some(turn),
            digest,
            1,
            1,
            transcript_digest,
            false,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            revision,
            1,
            Some(turn),
            digest,
            ProjectionLifecycle::Current,
        )),
    ]);
    commit(store, storage, batch(records));
    (
        turn,
        selected,
        CurrentTranscriptEntryProof::new(TranscriptGeneration::FIRST, TranscriptPosition::FIRST),
    )
}

fn assert_exact_replacement_intent(
    draft: &DraftRecord,
    turn: SyndicTurnId,
    selected: SelectedPathProof,
    entry: CurrentTranscriptEntryProof,
) {
    let DraftSubmissionIntent::Replacement(intent) = draft.submission_intent() else {
        panic!("replacement intent did not roundtrip");
    };
    assert_eq!(intent.target_turn_id(), turn);
    assert_eq!(intent.selected_path(), selected);
    assert_eq!(intent.transcript_entry(), entry);
}

#[test]
fn replacement_intent_roundtrips_with_exact_selected_path_proof() {
    let home = TestHome::new("replacement-roundtrip");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(90);
    let (turn, selected, entry) =
        seed_local_user_replacement_target(&store, storage, thread, draft_id(91));
    let current_draft = storage
        .current_draft(
            &store,
            thread,
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap()
        .draft()
        .id();
    commit(
        &store,
        storage,
        replacement_mutation(&store, storage, thread, turn, selected, entry),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let draft = storage
        .draft(
            &store,
            current_draft,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_exact_replacement_intent(&draft, turn, selected, entry);
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register_with_schema_validation(&mut reopened).unwrap();
    let draft = storage
        .draft(
            &reopened,
            current_draft,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_exact_replacement_intent(&draft, turn, selected, entry);
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn replacement_intent_rejects_stale_proof_and_wrong_entry_target() {
    exercise_seeded_populated_case(
        "replacement-selected-proof",
        "replacement edit selected-path proof disagrees with current thread",
        |store, storage| {
            let (turn, _selected, entry) = seed_real_replacement_target(store, storage);
            let current = storage
                .current_draft(store, id(30), SyndicPointReadLimit::new(1_000_000).unwrap())
                .unwrap()
                .unwrap();
            let selected = SelectedPathProof::new(
                None,
                current.thread().revision(),
                empty_selected_path_digest(),
            );
            replacement_mutation(store, storage, id(30), turn, selected, entry)
        },
    );
    exercise_seeded_populated_case(
        "replacement-off-path",
        "replacement edit transcript entry or user item disagrees",
        |store, storage| {
            let (turn, selected, entry) = seed_real_replacement_target(store, storage);
            replacement_mutation(
                store,
                storage,
                id(30),
                turn,
                selected,
                CurrentTranscriptEntryProof::new(entry.generation(), TranscriptPosition::FIRST),
            )
        },
    );
    exercise_seeded_populated_case(
        "replacement-empty-path",
        "replacement edit target requires a selected path",
        |store, storage| {
            let thread = id(80);
            let target = SyndicTurnId::from_bytes([82; 16]);
            seed_empty_replacement_thread(store, storage, thread, draft_id(81));
            let current = storage
                .current_draft(store, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
                .unwrap()
                .unwrap();
            let mut corrupt = replacement_mutation(
                store,
                storage,
                thread,
                target,
                SelectedPathProof::new(
                    None,
                    current.thread().revision(),
                    empty_selected_path_digest(),
                ),
                CurrentTranscriptEntryProof::new(
                    TranscriptGeneration::FIRST,
                    TranscriptPosition::FIRST,
                ),
            );
            corrupt
                .put(FixtureRecord::Turn(TurnRecord::new(
                    target,
                    thread,
                    TurnKind::OrdinaryUser,
                    ConversationParent::Root,
                    None,
                    TurnDepth::FIRST,
                    root_turn_chain_digest(target),
                    timestamp(2),
                )))
                .unwrap();
            corrupt
                .put(FixtureRecord::TurnState(fixture_turn_state(
                    target,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Interrupted,
                    0,
                    0,
                    timestamp(2),
                )))
                .unwrap();
            corrupt
        },
    );
}

#[test]
fn replacement_intent_rejects_provider_operation_target() {
    let target = SyndicTurnId::from_bytes([52; 16]);
    let digest = root_turn_chain_digest(target);
    exercise_seeded_populated_case(
        "replacement-provider-operation",
        "replacement edit target is not an ordinary user turn",
        |store, storage| {
            let (_turn, selected, entry) = seed_real_replacement_target(store, storage);
            let mut corrupt = replacement_mutation(store, storage, id(30), target, selected, entry);
            corrupt
                .put(FixtureRecord::Turn(TurnRecord::new(
                    target,
                    id(30),
                    TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction),
                    ConversationParent::Root,
                    None,
                    TurnDepth::FIRST,
                    digest,
                    timestamp(14),
                )))
                .unwrap();
            corrupt
                .put(FixtureRecord::TurnState(fixture_turn_state(
                    target,
                    TurnStateRevision::FIRST,
                    TurnLifecycle::Interrupted,
                    0,
                    0,
                    timestamp(14),
                )))
                .unwrap();
            corrupt
        },
    );
}

#[test]
fn replacement_validation_uses_one_exact_current_entry_instead_of_ancestry_walk() {
    exercise_seeded_populated_case(
        "replacement-wrong-current-entry",
        "replacement edit transcript entry or user item disagrees",
        |store, storage| {
            let (turn, selected, entry) = seed_real_replacement_target(store, storage);
            replacement_mutation(
                store,
                storage,
                id(30),
                turn,
                selected,
                CurrentTranscriptEntryProof::new(entry.generation(), TranscriptPosition::FIRST),
            )
        },
    );
}
