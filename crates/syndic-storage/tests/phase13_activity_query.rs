#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandError, CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{CasItemId, SyndicItemId};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
use syndic_storage::*;

use support::{
    batch, commit, converge_and_release_terminal_history, draft_id,
    exact_cas::{
        admit_event, admit_started_then_completed_item, correlate_user_item, establish_turn,
        submit_current_draft,
    },
    id, open, populated, timestamp, TestHome,
};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

#[test]
fn child_handoff_uses_exact_owner_source_range_and_revision_bound_source_pages() {
    let home = TestHome::new("phase13-activity-child-handoff");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    populated::seed_populated(&store, storage);
    let owner = id(30);
    let child = id(36);
    converge_and_release_terminal_history(&store, storage, owner, populated::source_turn());
    submit_current_draft(
        &store,
        storage,
        owner,
        draft_id(200),
        SyndicItemId::from_bytes([201; 16]),
        "owner continues",
        timestamp(10),
    );
    let submitted_child_item = SyndicItemId::from_bytes([202; 16]);
    let child_turn = submit_current_draft(
        &store,
        storage,
        child,
        draft_id(203),
        submitted_child_item,
        "child question",
        timestamp(11),
    );
    let source = establish_turn(&store, storage, child, child_turn, timestamp(12));
    admit_event(
        &store,
        storage,
        child,
        child_turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(12),
    );
    correlate_user_item(
        &store,
        storage,
        child,
        child_turn,
        submitted_child_item,
        &source,
        timestamp(13),
    );
    let final_answer = SyndicItemId::from_bytes([204; 16]);
    let answer = ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline("child final"),
        phase: Some(ProviderMessagePhaseV1::FinalAnswer),
        memory_citation: None,
    });
    admit_started_then_completed_item(
        &store,
        storage,
        child,
        child_turn,
        final_answer,
        &source,
        CasItemId::new("phase13-child-early-final-answer").unwrap(),
        answer.clone(),
        answer,
        timestamp(14),
        timestamp(15),
    );
    let head = storage
        .activity_query_head(&store, owner, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    let nonterminal = PublishActivityChildHandoff::new(
        owner,
        head.revision(),
        child,
        child_turn,
        final_answer,
        ProjectionSourceRange::new(0, 11).unwrap(),
    );
    let error = match execute(
        &store,
        storage.publish_activity_child_handoff(storage.revision(&store).unwrap(), nonterminal),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected not-committed nonterminal handoff, got {outcome:?}"),
    };
    let CommandError::ContributorValidation {
        source: error_source,
        ..
    } = error
    else {
        panic!("expected nonterminal handoff rejection")
    };
    assert!(matches!(
        error_source.downcast_ref::<SyndicMutationError>(),
        Some(SyndicMutationError::ActivityQueryConflict)
    ));
    admit_event(
        &store,
        storage,
        child,
        child_turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(16),
    );
    let preexisting_head = ActivityQueryHeadRecord::new(
        head.thread_id(),
        head.work_period(),
        head.source(),
        head.source_active(),
        6,
        head.revision(),
        2,
        head.logical_row_count(),
        head.running_row_count(),
        head.completed_row_count(),
        head.completed_stored_bytes(),
        head.completed_retention_cutoff(),
        head.lifecycle(),
    )
    .unwrap();
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::ActivityQueryHead(preexisting_head),
            FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
                owner,
                head.work_period(),
                ActivityQuerySource::new(child, child_turn),
                None,
                6,
                false,
                None,
            )),
        ]),
    );
    let existing = PublishActivityChildHandoff::new(
        owner,
        head.revision(),
        child,
        child_turn,
        final_answer,
        ProjectionSourceRange::new(0, 11).unwrap(),
    );
    let error = match execute(
        &store,
        storage.publish_activity_child_handoff(storage.revision(&store).unwrap(), existing),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected not-committed preexisting handoff, got {outcome:?}"),
    };
    let CommandError::ContributorValidation {
        source: error_source,
        ..
    } = error
    else {
        panic!("expected preexisting child membership rejection")
    };
    assert!(matches!(
        error_source.downcast_ref::<SyndicMutationError>(),
        Some(SyndicMutationError::ActivityQueryConflict)
    ));
    let mut restore = FixtureBatch::new();
    restore
        .put(FixtureRecord::ActivityQueryHead(head.clone()))
        .unwrap();
    restore
        .delete(FixtureDelete::ActivityQuerySource {
            thread: owner,
            work_period: head.work_period(),
            source_thread: child,
            source_turn: child_turn,
        })
        .unwrap();
    match execute(
        &store,
        storage.fixture_contribution(storage.revision(&store).unwrap(), restore),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed activity fixture restoration, got {outcome:?}"),
    }
    let invalid = PublishActivityChildHandoff::new(
        owner,
        head.revision(),
        child,
        child_turn,
        final_answer,
        ProjectionSourceRange::new(0, 12).unwrap(),
    );
    let error = match execute(
        &store,
        storage.publish_activity_child_handoff(storage.revision(&store).unwrap(), invalid),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected not-committed invalid handoff, got {outcome:?}"),
    };
    let CommandError::ContributorValidation {
        source: error_source,
        ..
    } = error
    else {
        panic!("expected handoff validation rejection")
    };
    assert!(matches!(
        error_source.downcast_ref::<SyndicMutationError>(),
        Some(SyndicMutationError::ActivityQueryConflict)
    ));

    let range = ProjectionSourceRange::new(0, 11).unwrap();
    let request = PublishActivityChildHandoff::new(
        owner,
        head.revision(),
        child,
        child_turn,
        final_answer,
        range,
    );
    match execute(
        &store,
        storage.publish_activity_child_handoff(storage.revision(&store).unwrap(), request),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed activity handoff, got {outcome:?}"),
    }
    let published = storage
        .activity_query_head(&store, owner, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(published.source_count(), 2);
    assert_eq!(published.source_frontier(), 6);
    assert_eq!(published.completed_row_count(), 1);
    let page = storage
        .activity_query_page(
            &store,
            &published,
            None,
            CursorReadLimits::new(4, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(
        page.stored_bytes() as u64,
        published.completed_stored_bytes()
    );
    let entry = &page.records()[0];
    assert_eq!(entry.thread_id(), owner);
    assert_eq!(entry.source().thread_id(), child);
    assert_eq!(entry.source().turn_id(), child_turn);
    assert_eq!(entry.source_event(), SourceEventSequence::new(5).unwrap());
    let Some(ActivityCompactFact::ChildHandoff(fact)) = entry.compact_fact() else {
        panic!("published child handoff must carry its compact fact")
    };
    assert_eq!(fact.observed_child_thread_id(), child);
    assert_eq!(fact.final_answer_range(), range);
    assert_eq!(fact.byte_count(), 11);

    let first_sources = storage
        .activity_query_source_page(
            &store,
            &published,
            None,
            CursorReadLimits::new(1, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(first_sources.records().len(), 1);
    let old_cursor = first_sources.next_cursor().unwrap();
    let second_sources = storage
        .activity_query_source_page(
            &store,
            &published,
            Some(old_cursor),
            CursorReadLimits::new(1, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(second_sources.records().len(), 1);
    assert!(second_sources.next_cursor().is_none());
    let child_source = [first_sources.records(), second_sources.records()]
        .into_iter()
        .flatten()
        .find(|record| record.source().thread_id() == child)
        .unwrap();
    assert_eq!(
        child_source.activity_start(),
        Some(SourceEventSequence::new(5).unwrap())
    );
    assert_eq!(child_source.source_frontier(), 6);
    assert!(!child_source.active());
    assert_eq!(
        child_source.child_handoff().unwrap().final_answer_range(),
        range
    );

    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let clean_sources = storage
        .activity_query_source_page(
            &store,
            &published,
            None,
            CursorReadLimits::new(4, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(clean_sources.records().len(), 2);
    assert!(clean_sources
        .records()
        .iter()
        .find(|record| record.source().thread_id() == child)
        .is_some_and(|record| !record.active()));

    let revised = ActivityQueryHeadRecord::new(
        published.thread_id(),
        published.work_period(),
        published.source(),
        published.source_active(),
        published.source_frontier(),
        published.revision().checked_next().unwrap(),
        published.source_count(),
        published.logical_row_count(),
        published.running_row_count(),
        published.completed_row_count(),
        published.completed_stored_bytes(),
        published.completed_retention_cutoff(),
        published.lifecycle(),
    )
    .unwrap();
    commit(
        &store,
        storage,
        batch([FixtureRecord::ActivityQueryHead(revised.clone())]),
    );
    assert!(matches!(
        storage.activity_query_source_page(
            &store,
            &revised,
            Some(old_cursor),
            CursorReadLimits::new(1, 65_536).unwrap(),
        ),
        Err(SyndicReadError::InvalidActivityQueryCursor)
    ));

    let mut corruption = FixtureBatch::new();
    corruption
        .delete(FixtureDelete::ActivityQuerySource {
            thread: owner,
            work_period: revised.work_period(),
            source_thread: child,
            source_turn: child_turn,
        })
        .unwrap();
    match execute(
        &store,
        storage.fixture_contribution(storage.revision(&store).unwrap(), corruption),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            reconciliation.install();
            panic!("expected clean activity-source corruption");
        }
        outcome => panic!("expected clean activity-source corruption, got {outcome:?}"),
    }
    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("activity-query source authority disagrees"),
        "unexpected scrub error: {error}"
    );
}
