#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CursorReadLimits, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AdmissionMarkers, AdvanceItemProjectionBuild, AdvanceTranscriptBuild, CasTurnSource,
    ComposerAtom, ComposerPayload, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    FinalizeNextTurnItem, HistorySummaryRecord, IdleSubmission, ItemProjectionGeneration,
    PreparedContent, ProjectionLifecycle, ProjectionOrdinal, SourceEventPayload,
    StartItemProjectionBuild, StartTranscriptBuild, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp, TranscriptBuildPhase, TranscriptBuildRecord, TranscriptGeneration,
    TranscriptPosition, TranscriptViewEntryRecord, TranscriptViewHeadRecord, TurnDepth,
    TurnEndStatus, TurnItemOrdinal, test_faults::fixture_advance_transcript_digest,
};

use support::{
    TestHome, draft_id,
    exact_cas::{admit_event, correlate_user_item, establish_turn},
    id, open, stage_prepared_content, timestamp,
};

const READ_BYTES: usize = 1_000_000;

#[derive(Clone, Copy)]
struct SubmittedTurn {
    turn: SyndicTurnId,
    item: SyndicItemId,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(READ_BYTES).unwrap()
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn command(store: &HomeStore, contribution: MutationContribution) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    command
}

fn execute(store: &HomeStore, contribution: MutationContribution) {
    store.execute(command(store, contribution)).unwrap();
}

fn create_thread(store: &HomeStore, storage: SyndicStorage) -> SyndicThreadId {
    let thread = id(1);
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(thread, draft_id(2), timestamp(1)),
        ),
    );
    thread
}

fn submit_turn(store: &HomeStore, storage: SyndicStorage, thread: SyndicThreadId) -> SubmittedTurn {
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("publish atomically").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &content);

    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => panic!("fixture draft must change"),
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );

    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let item = SyndicItemId::from_bytes([20; 16]);
    let submission = IdleSubmission::new(
        thread,
        thread_record.record().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.record().revision(),
        draft_id(3),
        item,
        AdmissionMarkers::default(),
        timestamp(3),
    );
    let turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    );
    SubmittedTurn { turn, item }
}

fn admit(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) {
    admit_event(store, storage, thread, turn, source, payload, observed_at);
}

fn project_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.record().revision(), generation),
        ),
    );
    for _ in 0..1_024 {
        if storage
            .item_projection_set(store, item, generation, point_limit())
            .unwrap()
            .is_some()
        {
            return;
        }
        let build = storage
            .item_projection_build(store, item, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.record().revision()),
            ),
        );
    }
    panic!("bounded item projection did not finish");
}

fn complete_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    submitted: SubmittedTurn,
) {
    let source = establish_turn(store, storage, thread, submitted.turn, timestamp(4));
    admit(
        store,
        storage,
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_user_item(
        store,
        storage,
        thread,
        submitted.turn,
        submitted.item,
        &source,
        timestamp(4),
    );
    admit(
        store,
        storage,
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(5),
    );
    project_item(store, storage, submitted.item);
    let state = storage
        .turn_state(store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.finalize_next_turn_item(
            storage.revision(store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                submitted.turn,
                state.record().revision(),
                TurnItemOrdinal::FIRST,
                submitted.item,
                timestamp(6),
            ),
        ),
    );
}

#[derive(Clone)]
struct PublicationTarget {
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
    expected_entry: TranscriptViewEntryRecord,
}

fn prepare_final_publication(store: &HomeStore, storage: SyndicStorage) -> PublicationTarget {
    let thread = create_thread(store, storage);
    let submitted = submit_turn(store, storage, thread);
    complete_turn(store, storage, thread, submitted);

    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.record().lifecycle(), ProjectionLifecycle::Stale);
    let generation = head.record().generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(
                thread,
                thread_record.record().revision(),
                head.record().revision(),
            ),
        ),
    );
    let collecting = storage
        .transcript_build(store, thread, generation, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.advance_transcript_build(
            storage.revision(store).unwrap(),
            AdvanceTranscriptBuild::new(thread, generation, collecting.record().revision()),
        ),
    );

    let ready = storage
        .transcript_build(store, thread, generation, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        ready.record().phase(),
        TranscriptBuildPhase::Publishing {
            next_depth: TurnDepth::FIRST,
            next_item: TurnItemOrdinal::FIRST,
            next_projection: ProjectionOrdinal::FIRST,
        }
    );
    let canonical = storage
        .canonical_item(store, submitted.item, point_limit())
        .unwrap()
        .unwrap();
    let item_head = storage
        .item_projection_head(store, submitted.item, point_limit())
        .unwrap()
        .unwrap();
    let projections = storage
        .item_projections(
            store,
            submitted.item,
            item_head.record().generation(),
            None,
            CursorReadLimits::new(2, READ_BYTES).unwrap(),
        )
        .unwrap();
    assert_eq!(projections.records().len(), 1);
    assert!(!projections.has_more());
    let projection = &projections.records()[0];
    let expected_entry = TranscriptViewEntryRecord::new(
        thread,
        generation,
        TranscriptPosition::FIRST,
        submitted.item,
        canonical.record().revision(),
        item_head.record().generation(),
        projection.projection_id(),
        projection.projection_revision(),
    );
    PublicationTarget {
        thread,
        generation,
        expected_entry,
    }
}

#[derive(Debug, Eq, PartialEq)]
struct PublicationSnapshot {
    build: TranscriptBuildRecord,
    head: TranscriptViewHeadRecord,
    summary: HistorySummaryRecord,
    entries: Vec<TranscriptViewEntryRecord>,
}

fn observe(
    store: &HomeStore,
    storage: SyndicStorage,
    target: &PublicationTarget,
) -> PublicationSnapshot {
    let build = *storage
        .transcript_build(store, target.thread, target.generation, point_limit())
        .unwrap()
        .unwrap()
        .record();
    let head = storage
        .transcript_view_head(store, target.thread, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    let summary = storage
        .history_summary(store, target.thread, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    let page = storage
        .transcript_entries(
            store,
            target.thread,
            target.generation,
            None,
            CursorReadLimits::new(2, READ_BYTES).unwrap(),
        )
        .unwrap();
    assert!(!page.has_more());
    PublicationSnapshot {
        build,
        head,
        summary,
        entries: page.records().to_vec(),
    }
}

fn expected_published(
    unpublished: &PublicationSnapshot,
    target: &PublicationTarget,
) -> PublicationSnapshot {
    let build = unpublished.build;
    let revision = build.revision().checked_next().unwrap();
    let entry_digest =
        fixture_advance_transcript_digest(build.entry_digest(), &target.expected_entry);
    PublicationSnapshot {
        build: TranscriptBuildRecord::new(
            build.thread_id(),
            build.generation(),
            revision,
            build.source_thread_revision(),
            build.committed_tail(),
            build.selected_path_digest(),
            build.path_turn_count(),
            1,
            entry_digest,
            build.history_complete(),
            TranscriptBuildPhase::Complete,
        ),
        head: TranscriptViewHeadRecord::new(
            build.thread_id(),
            build.generation(),
            revision,
            1,
            build.committed_tail(),
            build.selected_path_digest(),
            ProjectionLifecycle::Current,
        ),
        summary: HistorySummaryRecord::new(
            build.thread_id(),
            build.source_thread_revision(),
            build.committed_tail(),
            build.selected_path_digest(),
            build.history_complete(),
            unpublished.summary.last_activity_at(),
        ),
        entries: vec![target.expected_entry.clone()],
    }
}

#[derive(Clone, Copy)]
enum ExpectedState {
    Unpublished,
    Published,
    Either,
}

fn assert_state(
    observed: &PublicationSnapshot,
    unpublished: &PublicationSnapshot,
    published: &PublicationSnapshot,
    expected: ExpectedState,
) {
    let is_unpublished = observed == unpublished;
    let is_published = observed == published;
    assert!(
        is_unpublished || is_published,
        "recovery exposed a mixed final transcript publication: {observed:#?}"
    );
    match expected {
        ExpectedState::Unpublished => {
            assert!(is_unpublished, "before-commit cut published new state")
        }
        ExpectedState::Published => {
            assert!(is_published, "post-persist cut lost published state")
        }
        ExpectedState::Either => {}
    }
}

#[test]
fn final_transcript_publication_cuts_reconcile_as_one_atomic_state() {
    for (name, point, expected) in [
        (
            "phase7-transcript-final-before-commit",
            FaultPoint::BeforeCommit,
            ExpectedState::Unpublished,
        ),
        (
            "phase7-transcript-final-after-commit-before-persist",
            FaultPoint::AfterCommitBeforePersist,
            ExpectedState::Either,
        ),
        (
            "phase7-transcript-final-after-persist",
            FaultPoint::AfterPersist,
            ExpectedState::Published,
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let target = prepare_final_publication(&store, storage);
        let unpublished = observe(&store, storage, &target);
        assert!(unpublished.entries.is_empty() && unpublished.build.entry_count() == 0);
        assert!(
            unpublished.head.entry_count() == 0
                && unpublished.head.lifecycle() == ProjectionLifecycle::Stale
        );
        assert!(!unpublished.summary.complete());
        assert!(unpublished.build.history_complete());
        let published = expected_published(&unpublished, &target);

        let contribution = storage.advance_transcript_build(
            storage.revision(&store).unwrap(),
            AdvanceTranscriptBuild::new(
                target.thread,
                target.generation,
                unpublished.build.revision(),
            ),
        );
        let command = command(&store, contribution);
        faults.fail_next(point);
        assert!(store.execute(command).is_err());
        assert_eq!(store.health().state(), HomeHealthState::Verifying);

        store.verify_health().unwrap();
        let recovered = observe(&store, storage, &target);
        assert_state(&recovered, &unpublished, &published, expected);
        store.validate_registered_domains().unwrap();
        store.close().unwrap();

        let mut reopened = open(home.path());
        let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
        let durable = observe(&reopened, reopened_storage, &target);
        assert_eq!(durable, recovered);
        assert_state(&durable, &unpublished, &published, expected);
        reopened.validate_registered_domains().unwrap();
        reopened.close().unwrap();
    }
}
