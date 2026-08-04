use super::*;

use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};

fn apply_fixture(store: &HomeStore, storage: SyndicStorage, fixture: FixtureBatch) {
    execute(
        store,
        storage.fixture_contribution(storage.revision(store).unwrap(), fixture),
    )
    .unwrap();
}

fn expect_reopen_rejection(home: &TestHome, store: HomeStore, expected: &str) {
    store.close().unwrap();
    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("coherently corrupted activity projection reopened successfully"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains(expected),
        "unexpected reopen error: {error}"
    );
}

fn one_completed_activity(
    name: &str,
) -> (
    TestHome,
    HomeStore,
    SyndicStorage,
    SyndicThreadId,
    ActivityQueryHeadRecord,
    ActivityQueryEntryRecord,
) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_submitted_user_item(&store, storage, thread, turn, &source, timestamp(5));
    let item = SyndicItemId::from_bytes([210; 16]);
    let cas = CasItemId::new("phase6-corruption-completed").unwrap();
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        item,
        &source,
        command_start(cas.clone(), timestamp(6)),
        timestamp(6),
    );
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        item,
        &source,
        command_completion(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas,
            "done",
            timestamp(7),
        ),
        timestamp(7),
    );
    let head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    let page = storage
        .activity_query_page(
            &store,
            &head,
            None,
            CursorReadLimits::new(2, 65_536).unwrap(),
        )
        .unwrap();
    (
        home,
        store,
        storage,
        thread,
        head,
        page.records()[0].clone(),
    )
}

#[test]
fn reopen_rejects_a_coherently_removed_running_activity_entry() {
    let home = TestHome::new("phase6-activity-running-completeness-corruption");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_submitted_user_item(&store, storage, thread, turn, &source, timestamp(5));
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        SyndicItemId::from_bytes([211; 16]),
        &source,
        command_start(
            CasItemId::new("phase6-corruption-running").unwrap(),
            timestamp(6),
        ),
        timestamp(6),
    );
    let head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    let page = storage
        .activity_query_page(
            &store,
            &head,
            None,
            CursorReadLimits::new(2, 65_536).unwrap(),
        )
        .unwrap();
    let mut corruption = FixtureBatch::new();
    corruption
        .delete(FixtureDelete::ActivityQueryEntry {
            thread,
            work_period: head.work_period(),
            order: page.records()[0].order(),
        })
        .unwrap();
    corruption
        .put(FixtureRecord::ActivityQueryHead(
            ActivityQueryHeadRecord::new(
                thread,
                head.work_period(),
                head.source(),
                head.source_active(),
                head.source_frontier(),
                head.revision(),
                head.source_count(),
                0,
                0,
                0,
                0,
                None,
                head.lifecycle(),
            )
            .unwrap(),
        ))
        .unwrap();
    apply_fixture(&store, storage, corruption);
    expect_reopen_rejection(&home, store, "running source item has no exact entry");
}

#[test]
fn reopen_rejects_coherently_removed_completed_retention_that_still_fits() {
    let (home, store, storage, thread, head, entry) =
        one_completed_activity("phase6-activity-completed-completeness-corruption");
    let mut corruption = FixtureBatch::new();
    corruption
        .delete(FixtureDelete::ActivityQueryEntry {
            thread,
            work_period: head.work_period(),
            order: entry.order(),
        })
        .unwrap();
    corruption
        .put(FixtureRecord::ActivityQueryHead(
            ActivityQueryHeadRecord::new(
                thread,
                head.work_period(),
                head.source(),
                head.source_active(),
                head.source_frontier(),
                head.revision(),
                head.source_count(),
                0,
                0,
                0,
                0,
                None,
                head.lifecycle(),
            )
            .unwrap(),
        ))
        .unwrap();
    apply_fixture(&store, storage, corruption);
    expect_reopen_rejection(&home, store, "completed retention is not maximal");
}

#[test]
fn reopen_rejects_a_coherently_advanced_activity_start() {
    let (home, store, storage, _thread, head, _entry) =
        one_completed_activity("phase6-activity-start-corruption");
    let page = storage
        .activity_query_source_page(
            &store,
            &head,
            None,
            CursorReadLimits::new(2, 65_536).unwrap(),
        )
        .unwrap();
    let source = &page.records()[0];
    assert_eq!(
        source.activity_start(),
        Some(SourceEventSequence::new(4).unwrap())
    );
    let mut corruption = FixtureBatch::new();
    corruption
        .put(FixtureRecord::ActivityQuerySource(
            ActivityQuerySourceRecord::new(
                source.thread_id(),
                source.work_period(),
                source.source(),
                Some(SourceEventSequence::new(5).unwrap()),
                source.source_frontier(),
                source.active(),
                source.child_handoff(),
            ),
        ))
        .unwrap();
    apply_fixture(&store, storage, corruption);
    expect_reopen_rejection(&home, store, "source activity start disagrees");
}

#[test]
fn reopen_rejects_an_inexact_retired_source_frontier() {
    let (home, store, storage, thread, active_head, _entry) =
        one_completed_activity("phase6-activity-retired-source-corruption");
    let turn = active_head.source().unwrap().turn_id();
    let source = storage
        .source_event(&store, turn, SourceEventSequence::FIRST, limit())
        .unwrap()
        .unwrap()
        .source()
        .unwrap()
        .clone();
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(8),
    );
    let terminal = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    converge_and_release_terminal_history(&store, storage, thread, turn);
    let payload = ComposerPayload::new(vec![ComposerAtom::text("next").unwrap()]).unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(&store, storage, &prepared);
    let current = storage
        .current_draft(&store, thread, limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, timestamp(9)).unwrap()
    else {
        panic!("rollover draft must become nonempty")
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    )
    .unwrap();
    let current = storage
        .current_draft(&store, thread, limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, thread, limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.submit_idle_draft(
            storage.revision(&store).unwrap(),
            IdleSubmission::new(
                thread,
                current.thread().revision(),
                current.draft().id(),
                current.draft().revision(),
                current.draft().content(),
                gate.revision(),
                draft_id(212),
                SyndicItemId::from_bytes([213; 16]),
                None,
                timestamp(10),
            ),
        ),
    )
    .unwrap();
    let mut corruption = FixtureBatch::new();
    corruption
        .put(FixtureRecord::ActivityQuerySource(
            ActivityQuerySourceRecord::new(
                thread,
                terminal.work_period(),
                terminal.source().unwrap(),
                Some(SourceEventSequence::new(4).unwrap()),
                terminal.source_frontier() - 1,
                false,
                None,
            ),
        ))
        .unwrap();
    apply_fixture(&store, storage, corruption);
    expect_reopen_rejection(&home, store, "source membership authority disagrees");
}
