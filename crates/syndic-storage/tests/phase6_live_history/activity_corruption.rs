use super::*;

use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};

fn apply_fixture(store: &HomeStore, storage: SyndicStorage, fixture: FixtureBatch) {
    assert_committed(execute(
        store,
        storage.fixture_contribution(storage.revision(store).unwrap(), fixture),
    ));
}

fn expect_scrub_rejection(home: &TestHome, store: HomeStore, expected: &str) {
    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error.to_string().contains(expected),
        "unexpected scrub error: {error}"
    );
    reopened.close().unwrap();
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
fn scrub_rejects_a_coherently_removed_running_activity_entry() {
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
    expect_scrub_rejection(&home, store, "running source item has no exact entry");
}

#[test]
fn scrub_rejects_coherently_removed_completed_retention_that_still_fits() {
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
    expect_scrub_rejection(&home, store, "completed retention is not maximal");
}

#[test]
fn scrub_rejects_a_coherently_advanced_activity_start() {
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
    expect_scrub_rejection(&home, store, "source activity start disagrees");
}

#[test]
fn scrub_rejects_an_inexact_retired_source_frontier() {
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
    submit_current_draft(
        &store,
        storage,
        thread,
        draft_id(212),
        SyndicItemId::from_bytes([213; 16]),
        "next",
        timestamp(10),
    );
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
    expect_scrub_rejection(&home, store, "source membership authority disagrees");
}
