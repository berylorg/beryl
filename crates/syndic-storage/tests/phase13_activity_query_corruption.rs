#![cfg(feature = "test-faults")]

#[path = "support/activity_handoff.rs"]
mod activity_handoff;
mod support;

use beryl_home_store::{CommandError, CursorReadLimits, HomeCommand};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureRecord, fixture_activity_query_entry_stored_bytes,
};
use syndic_storage::*;

use activity_handoff::{child_handoff_candidate, published_child_handoff};
use support::open;

#[test]
fn publication_rejects_a_final_answer_with_later_activity_before_terminal() {
    let candidate = child_handoff_candidate("phase13-activity-nonadjacent-handoff", true);
    let mut command = HomeCommand::new(candidate.store.home_revision().unwrap());
    command
        .add(candidate.storage.publish_activity_child_handoff(
            candidate.storage.revision(&candidate.store).unwrap(),
            PublishActivityChildHandoff::new(
                candidate.owner,
                candidate.head.revision(),
                candidate.child,
                candidate.child_turn,
                candidate.final_answer,
                ProjectionSourceRange::new(0, 11).unwrap(),
            ),
        ))
        .unwrap();
    let error = candidate.store.execute(command).unwrap_err();
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected non-terminal-adjacent handoff rejection")
    };
    assert!(matches!(
        source.downcast_ref::<SyndicMutationError>(),
        Some(SyndicMutationError::ActivityQueryConflict)
    ));
}

#[test]
fn reopen_rejects_handoff_entry_fact_that_disagrees_with_membership() {
    let fixture = published_child_handoff("phase13-activity-handoff-fact-corruption");
    let head = fixture.head;
    let page = fixture
        .storage
        .activity_query_page(
            &fixture.store,
            &head,
            None,
            CursorReadLimits::new(2, 65_536).unwrap(),
        )
        .unwrap();
    let original = &page.records()[0];
    assert_eq!(original.item_id(), fixture.final_answer);
    assert_eq!(original.source().thread_id(), fixture.child);
    assert_eq!(original.source().turn_id(), fixture.child_turn);
    let altered_range = ProjectionSourceRange::new(1, 10).unwrap();
    let corrupted = ActivityQueryEntryRecord::new(
        original.thread_id(),
        original.work_period(),
        original.order(),
        original.source().clone(),
        original.source_event(),
        original.provider_kind(),
        original.provider_lifecycle(),
        Some(ActivityCompactFact::ChildHandoff(
            ActivityChildHandoffFact::new(fixture.child, altered_range),
        )),
    )
    .unwrap();
    assert_eq!(
        fixture_activity_query_entry_stored_bytes(original),
        fixture_activity_query_entry_stored_bytes(&corrupted),
        "the coherent corruption must leave exact head byte counters unchanged"
    );
    let mut corruption = FixtureBatch::new();
    corruption
        .put(FixtureRecord::ActivityQueryEntry(corrupted))
        .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(fixture.storage.fixture_contribution(
            fixture.storage.revision(&fixture.store).unwrap(),
            corruption,
        ))
        .unwrap();
    fixture.store.execute(command).unwrap();
    fixture.store.close().unwrap();
    let mut reopened = open(fixture.home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("mismatched activity handoff fact reopened successfully"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("activity-query handoff entry disagrees with membership"),
        "unexpected reopen error: {error}"
    );
}
