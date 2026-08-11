#![cfg(feature = "test-faults")]

mod support;

#[path = "phase53_accepted_delivery/accepted_support.rs"]
mod accepted_support;

use beryl_home_store::{CommandOutcome, CursorReadLimits};
use beryl_model::{AcceptedInputRevision, InputGateRevision};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_route_leaf_with_transition,
};
use syndic_storage::*;

use accepted_support::{AcceptedOperation, large_ready_generation, seeded};
use support::{
    TestHome, batch, commit, id, open, phase11::mixed_abandonment_records,
    populated::populated_records,
};

fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, ACCEPTED_READY_PAGE_MAX_BYTES).unwrap()
}

fn source(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
) -> AcceptedReadySourceRecord {
    let revision = storage.revision(store).unwrap();
    let page = storage
        .accepted_ready_source_page(store, revision, None, limits(256))
        .unwrap();
    assert_eq!(page.source_revision(), revision);
    assert_eq!(page.records().len(), 1);
    page.records()[0]
}

#[test]
fn global_ready_source_pages_are_ordered_bounded_and_revision_fenced() {
    let (_home, store, storage) = seeded("phase57-ready-source-page", populated_records());
    let extra = AcceptedReadySourceRecord::new(
        id(50),
        InputGateRevision::new(1).unwrap(),
        AcceptedRouteGeneration::FIRST,
        AcceptedRouteRevision::FIRST,
        AcceptedInputOrdinal::FIRST,
        AcceptedInputOrdinal::FIRST,
    );
    commit(
        &store,
        storage,
        batch([FixtureRecord::AcceptedReadySource(extra)]),
    );
    let revision = storage.revision(&store).unwrap();
    let first = storage
        .accepted_ready_source_page(&store, revision, None, limits(1))
        .unwrap();
    assert_eq!(first.records().len(), 1);
    assert_eq!(first.records()[0].thread_id(), id(40));
    assert!(first.stored_bytes() <= ACCEPTED_READY_PAGE_MAX_BYTES);
    assert!(first.decoded_bytes() <= ACCEPTED_READY_PAGE_MAX_BYTES);
    let cursor = first.next_cursor().expect("second source remains");
    let second = storage
        .accepted_ready_source_page(&store, revision, Some(cursor), limits(1))
        .unwrap();
    assert_eq!(second.records(), &[extra]);
    assert!(second.next_cursor().is_none());

    let later = AcceptedReadySourceRecord::new(
        id(60),
        InputGateRevision::new(1).unwrap(),
        AcceptedRouteGeneration::FIRST,
        AcceptedRouteRevision::FIRST,
        AcceptedInputOrdinal::FIRST,
        AcceptedInputOrdinal::FIRST,
    );
    commit(
        &store,
        storage,
        batch([FixtureRecord::AcceptedReadySource(later)]),
    );
    assert!(matches!(
        storage.accepted_ready_source_page(&store, revision, Some(cursor), limits(1)),
        Err(SyndicReadError::StaleAcceptedReadySourceScan)
    ));
    assert!(matches!(
        storage.accepted_ready_source_page(
            &store,
            storage.revision(&store).unwrap(),
            Some(cursor),
            limits(1),
        ),
        Err(SyndicReadError::InvalidAcceptedReadySourceCursor)
    ));
}

fn terminal_heavy_records() -> Vec<FixtureRecord> {
    let mut records = large_ready_generation(384);
    let thread = id(40);
    let generation = AcceptedRouteGeneration::FIRST;
    for record in &mut records {
        match record {
            FixtureRecord::InputGate(gate) if gate.thread_id() == thread => {
                *gate = InputGateRecord::new(
                    thread,
                    gate.revision(),
                    gate.state().clone(),
                    384,
                    Some(generation),
                    gate.selected_route(),
                    84,
                    1,
                    0,
                )
                .unwrap();
            }
            FixtureRecord::AcceptedRouteGeneration(route) if route.thread_id() == thread => {
                *route = AcceptedRouteGenerationRecord::new(
                    thread,
                    generation,
                    route.revision(),
                    route.target().clone(),
                    Some(AcceptedInputOrdinal::FIRST),
                    Some(AcceptedInputOrdinal::new(384).unwrap()),
                    384,
                    84,
                    0,
                    1,
                    299,
                    0,
                    0,
                )
                .unwrap();
            }
            FixtureRecord::AcceptedRouteLeaf(leaf)
                if leaf.thread_id() == thread
                    && leaf.state() == AcceptedRouteLeafState::Routed
                    && leaf.ordinal().get() <= 300 =>
            {
                *leaf = fixture_route_leaf_with_transition(
                    AcceptedRouteLeafRecord::new(
                        leaf.input_id(),
                        leaf.thread_id(),
                        leaf.generation(),
                        leaf.ordinal(),
                        AcceptedInputRevision::new(3).unwrap(),
                        AcceptedRouteLeafState::Routed,
                        AcceptedInputLifecycle::Delivered,
                    ),
                    AcceptedRouteLeafTransitionProof::new(
                        InputGateRevision::new(leaf.ordinal().get() + 1).unwrap(),
                        AcceptedRouteHeadProof::new(
                            leaf.generation(),
                            AcceptedRouteRevision::FIRST,
                        ),
                        AcceptedInputRevision::new(2).unwrap(),
                        AcceptedRouteLeafTransitionKind::Complete,
                    ),
                );
            }
            _ => {}
        }
    }
    records
}

#[test]
fn candidate_pages_advance_across_terminal_history_and_preserve_accepted_order() {
    let (_home, store, storage) = seeded("phase57-terminal-heavy-ready", terminal_heavy_records());
    store.validate_registered_domains().unwrap();
    let source = source(&store, storage);
    let first = storage
        .accepted_ready_candidate_page(&store, source, None, limits(256))
        .unwrap();
    assert!(first.records().is_empty());
    assert!(first.stored_bytes() <= ACCEPTED_READY_PAGE_MAX_BYTES);
    assert!(first.decoded_bytes() <= ACCEPTED_READY_PAGE_MAX_BYTES);
    let cursor = first
        .next_cursor()
        .expect("terminal prefix must advance to the remaining source interval");

    let second = storage
        .accepted_ready_candidate_page(&store, source, Some(cursor), limits(256))
        .unwrap();
    assert_eq!(second.records().len(), 84);
    assert_eq!(second.records()[0].ordinal().get(), 301);
    assert_eq!(second.records().last().unwrap().ordinal().get(), 384);
    assert!(
        second
            .records()
            .windows(2)
            .all(|pair| pair[0].ordinal() < pair[1].ordinal())
    );
    assert!(
        second
            .records()
            .iter()
            .all(|candidate| candidate.lifecycle() == AcceptedInputLifecycle::Admitted)
    );
    assert!(second.next_cursor().is_none());
    assert!(second.stored_bytes() <= ACCEPTED_READY_PAGE_MAX_BYTES);
    assert!(second.decoded_bytes() <= ACCEPTED_READY_PAGE_MAX_BYTES);
}

#[test]
fn candidate_source_and_cursor_reject_route_revision_drift_separately() {
    let (_home, store, storage) =
        seeded("phase57-ready-candidate-drift", mixed_abandonment_records());
    let old_source = source(&store, storage);
    let first = storage
        .accepted_ready_candidate_page(&store, old_source, None, limits(1))
        .unwrap();
    assert_eq!(first.records().len(), 1);
    let old_cursor = first.next_cursor().expect("source interval continues");

    match store.execute_current(AcceptedOperation::Retry.current_command(storage)) {
        CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected retry command to commit without later failure, got {outcome:?}"),
    }
    let new_source = source(&store, storage);
    assert_ne!(new_source, old_source);
    assert!(matches!(
        storage.accepted_ready_candidate_page(&store, old_source, Some(old_cursor), limits(1),),
        Err(SyndicReadError::StaleAcceptedReadyCandidateSource)
    ));
    assert!(matches!(
        storage.accepted_ready_candidate_page(&store, new_source, Some(old_cursor), limits(1),),
        Err(SyndicReadError::InvalidAcceptedReadyCandidateCursor)
    ));
}

#[test]
fn claiming_the_last_ready_input_removes_global_source_authority() {
    let (_home, store, storage) = seeded("phase57-last-ready-claim", populated_records());
    assert_eq!(source(&store, storage).thread_id(), id(40));
    match store.execute_current(AcceptedOperation::Begin.current_command(storage)) {
        CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected begin command to commit without later failure, got {outcome:?}"),
    }
    let revision = storage.revision(&store).unwrap();
    let page = storage
        .accepted_ready_source_page(&store, revision, None, limits(256))
        .unwrap();
    assert!(page.records().is_empty());
    assert!(page.next_cursor().is_none());
    store.validate_registered_domains().unwrap();
}

fn assert_reopen_rejects(
    name: &str,
    corruption: impl FnOnce(SyndicStorage, &beryl_home_store::HomeStore),
    expected: &str,
) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    corruption(storage, &store);
    let error = store.validate_registered_domains().unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("accepted-ready corruption survived reopen"),
        Err(error) => error,
    };
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn reopen_validates_ready_source_relationship_in_both_directions() {
    assert_reopen_rejects(
        "phase57-missing-ready-source",
        |storage, store| {
            let mut deletion = FixtureBatch::new();
            deletion
                .delete(FixtureDelete::AcceptedReadySource {
                    thread: id(40),
                    generation: AcceptedRouteGeneration::FIRST,
                })
                .unwrap();
            commit(store, storage, deletion);
        },
        "accepted-route generation and ready-source authority disagree",
    );
    assert_reopen_rejects(
        "phase57-orphan-ready-source",
        |storage, store| {
            commit(
                store,
                storage,
                batch([FixtureRecord::AcceptedReadySource(
                    AcceptedReadySourceRecord::new(
                        id(90),
                        InputGateRevision::new(1).unwrap(),
                        AcceptedRouteGeneration::FIRST,
                        AcceptedRouteRevision::FIRST,
                        AcceptedInputOrdinal::FIRST,
                        AcceptedInputOrdinal::FIRST,
                    ),
                )]),
            );
        },
        "accepted-ready source references a missing generation",
    );
}
