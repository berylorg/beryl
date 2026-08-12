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

use accepted_support::{
    AcceptedOperation, seed_large_ready_generation, seeded_mixed, seeded_populated,
};
use support::{TestHome, batch, commit, id, open, populated::seed_populated};

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
    let (_home, store, storage) = seeded_populated("phase57-ready-source-page");
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

fn seed_terminal_heavy(store: &beryl_home_store::HomeStore, storage: SyndicStorage) {
    seed_large_ready_generation(store, storage, 384);
    let thread = id(40);
    let generation = AcceptedRouteGeneration::FIRST;
    let gate = storage
        .input_gate(store, thread, accepted_support::limit())
        .unwrap()
        .unwrap();
    let route =
        syndic_storage::test_faults::accepted_route_generation(store, storage, thread, generation)
            .unwrap();
    let mut leaves = Vec::new();
    let mut cursor = None;
    loop {
        let page = storage
            .accepted_route_page(store, thread, generation, route.revision(), cursor)
            .unwrap();
        for entry in page.records() {
            let leaf = entry.leaf();
            if leaf.ordinal().get() <= 300 && leaf.state() == AcceptedRouteLeafState::Routed {
                leaves.push(FixtureRecord::AcceptedRouteLeaf(
                    fixture_route_leaf_with_transition(
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
                    ),
                ));
            }
        }
        let Some(next_cursor) = page.next_cursor() else {
            break;
        };
        cursor = Some(next_cursor);
    }
    for chunk in leaves.chunks(96) {
        commit(store, storage, batch(chunk.iter().cloned()));
    }
    commit(
        store,
        storage,
        batch([
            FixtureRecord::InputGate(
                InputGateRecord::new(
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
                .unwrap(),
            ),
            FixtureRecord::AcceptedRouteGeneration(
                AcceptedRouteGenerationRecord::new(
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
                .unwrap(),
            ),
        ]),
    );
}

#[test]
fn candidate_pages_advance_across_terminal_history_and_preserve_accepted_order() {
    let home = TestHome::new("phase57-terminal-heavy-ready");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_terminal_heavy(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
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
    let (_home, store, storage) = seeded_mixed("phase57-ready-candidate-drift");
    let old_source = source(&store, storage);
    let first = storage
        .accepted_ready_candidate_page(&store, old_source, None, limits(1))
        .unwrap();
    assert_eq!(first.records().len(), 1);
    let old_cursor = first.next_cursor().expect("source interval continues");

    match store.execute_current(AcceptedOperation::Retry.current_command(storage)) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => {
            panic!("expected retry command to commit without later failure, got {outcome:?}")
        }
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
    let (_home, store, storage) = seeded_populated("phase57-last-ready-claim");
    assert_eq!(source(&store, storage).thread_id(), id(40));
    match store.execute_current(AcceptedOperation::Begin.current_command(storage)) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => {
            panic!("expected begin command to commit without later failure, got {outcome:?}")
        }
    }
    let revision = storage.revision(&store).unwrap();
    let page = storage
        .accepted_ready_source_page(&store, revision, None, limits(256))
        .unwrap();
    assert!(page.records().is_empty());
    assert!(page.next_cursor().is_none());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

fn assert_reopen_rejects(
    name: &str,
    corruption: impl FnOnce(SyndicStorage, &beryl_home_store::HomeStore),
    expected: &str,
) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    corruption(storage, &store);
    let error = store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
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
