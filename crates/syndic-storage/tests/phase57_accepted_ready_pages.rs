#![cfg(feature = "test-faults")]

mod support;

#[path = "phase53_accepted_delivery/fixtures.rs"]
mod accepted_fixtures;
#[path = "phase53_accepted_delivery/accepted_support.rs"]
mod accepted_support;

use beryl_home_store::{CommandOutcome, CursorReadLimits};
use beryl_model::{AcceptedInputRevision, InputGateRevision, SyndicItemId, SyndicThreadId};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_route_leaf_with_transition,
};
use syndic_storage::*;

use accepted_support::{
    AcceptedOperation, seed_large_ready_generation, seeded_mixed, seeded_populated,
};
use support::{
    TestHome, batch, commit, draft_id, empty_composer_content, id, open, populated::seed_populated,
    seed_canonical_empty_thread,
};

fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, ACCEPTED_READY_PAGE_MAX_BYTES).unwrap()
}

fn source(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
) -> AcceptedReadySourceRecord {
    let revision = storage.revision(store).unwrap();
    let page = storage
        .accepted_ready_source_page(store, revision, None, limits(256))
        .unwrap();
    assert_eq!(page.source_revision(), revision);
    assert_eq!(page.records().len(), 1);
    page.records()[0]
}

fn seed_ordinary_ready_source(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    draft_byte: u8,
) -> AcceptedReadySourceRecord {
    let initial_draft = draft_id(draft_byte);
    let replacement_draft = draft_id(draft_byte + 1);
    let consumed_source_draft = draft_id(draft_byte + 2);
    seed_canonical_empty_thread(store, storage.clone(), thread, initial_draft);
    let turn = support::exact_cas::submit_current_draft(
        store,
        storage.clone(),
        thread,
        replacement_draft,
        SyndicItemId::from_bytes([draft_byte; 16]),
        "ready-source backing turn",
        support::timestamp(10),
    );
    let _source = support::exact_cas::establish_turn(
        store,
        storage.clone(),
        thread,
        turn,
        support::timestamp(11),
    );

    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let current_thread = storage.thread(store, thread, limit).unwrap().unwrap();
    let current_draft = storage
        .current_draft(store, thread, limit)
        .unwrap()
        .unwrap();
    let history = storage
        .history_summary(store, thread, limit)
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(store, thread, limit).unwrap().unwrap();
    let route_head = gate.selected_route().expect("active route is selected");
    let route = syndic_storage::test_faults::accepted_route_generation(
        store,
        storage.clone(),
        thread,
        route_head.generation(),
    )
    .unwrap();
    assert!(matches!(gate.state(), InputGateState::Steerable(_)));
    assert!(matches!(route.target(), AcceptedRouteTarget::Steering(_)));

    let final_thread_revision = current_thread.revision().checked_next().unwrap();
    let final_gate_revision = gate.revision().checked_next().unwrap();
    let final_route_revision = route.revision().checked_next().unwrap();
    let final_head = AcceptedRouteHeadProof::new(route_head.generation(), final_route_revision);
    let input = consumed_source_draft.accepted_input_id();
    let source = AcceptedReadySourceRecord::new(
        thread,
        final_gate_revision,
        route_head.generation(),
        final_route_revision,
        AcceptedInputOrdinal::FIRST,
        AcceptedInputOrdinal::FIRST,
    );
    let content = empty_composer_content();
    commit(
        store,
        storage.clone(),
        batch([
            FixtureRecord::Thread(ThreadRecord::new(
                thread,
                SelectedPathProof::new(
                    current_thread.committed_tail(),
                    final_thread_revision,
                    current_thread.selected_path_digest(),
                ),
                current_thread.current_draft_id(),
                current_thread.lineage(),
                current_thread.context_owner_id(),
            )),
            FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                thread,
                current_draft.draft().id(),
                current_draft.draft().revision(),
                final_thread_revision,
            )),
            FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                thread,
                history.revision().checked_next().unwrap(),
                final_thread_revision,
                history.committed_tail(),
                history.selected_path_digest(),
                history.complete(),
                history.last_activity_at(),
            )),
            FixtureRecord::AcceptedInput(
                AcceptedInputRecord::new(
                    input,
                    thread,
                    AcceptedInputOrdinal::FIRST,
                    AcceptedInputAdmissionProof::new(
                        current_thread.revision(),
                        consumed_source_draft,
                        current_draft.draft().revision(),
                        gate.revision(),
                        current_draft.draft().id(),
                    )
                    .unwrap(),
                    route_head.generation(),
                    content,
                    None,
                    current_draft.draft().created_at(),
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                thread,
                AcceptedInputOrdinal::FIRST,
                input,
                route_head.generation(),
            )),
            FixtureRecord::AcceptedRouteGeneration(
                AcceptedRouteGenerationRecord::new(
                    thread,
                    route_head.generation(),
                    final_route_revision,
                    route.target().clone(),
                    Some(AcceptedInputOrdinal::FIRST),
                    Some(AcceptedInputOrdinal::FIRST),
                    1,
                    1,
                    0,
                    0,
                    0,
                    content.summary().logical_utf8_bytes(),
                    0,
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedRouteGenerationHead(AcceptedRouteGenerationHeadRecord::new(
                thread, final_head,
            )),
            FixtureRecord::AcceptedReadySource(source),
            FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
                input,
                thread,
                route_head.generation(),
                AcceptedInputOrdinal::FIRST,
                AcceptedInputRevision::new(1).unwrap(),
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Admitted,
            )),
            FixtureRecord::InputGate(
                InputGateRecord::new(
                    thread,
                    final_gate_revision,
                    gate.state().clone(),
                    AcceptedInputOrdinal::FIRST.get(),
                    Some(route_head.generation()),
                    Some(final_head),
                    1,
                    0,
                    content.summary().logical_utf8_bytes(),
                )
                .unwrap(),
            ),
        ]),
    );
    source
}

#[test]
fn global_ready_source_pages_are_ordered_bounded_and_revision_fenced() {
    let (_home, store, storage) = seeded_populated("phase57-ready-source-page");
    let extra = seed_ordinary_ready_source(&store, &storage, id(50), 150);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
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

    let _later = seed_ordinary_ready_source(&store, &storage, id(60), 160);
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
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

fn seed_terminal_heavy(store: &beryl_home_store::HomeStore, storage: &SyndicStorage) {
    seed_large_ready_generation(store, storage, 384);
    let thread = id(40);
    let generation = AcceptedRouteGeneration::FIRST;
    let initial_gate = storage
        .input_gate(store, thread, accepted_support::limit())
        .unwrap()
        .unwrap();
    let initial_route = syndic_storage::test_faults::accepted_route_generation(
        store,
        storage.clone(),
        thread,
        generation,
    )
    .unwrap();
    assert_eq!(
        initial_gate.selected_route(),
        Some(AcceptedRouteHeadProof::new(
            generation,
            initial_route.revision(),
        ))
    );

    let mut leaves = Vec::new();
    let mut cursor = None;
    loop {
        let page = storage
            .accepted_route_page(store, thread, generation, initial_route.revision(), cursor)
            .unwrap();
        for entry in page.records() {
            let leaf = entry.leaf();
            if leaf.ordinal().get() <= 300
                && leaf.state() == AcceptedRouteLeafState::Routed
                && leaf.lifecycle() == AcceptedInputLifecycle::Admitted
            {
                leaves.push(leaf.clone());
            }
        }
        let Some(next_cursor) = page.next_cursor() else {
            break;
        };
        cursor = Some(next_cursor);
    }

    assert_eq!(leaves.len(), 299);
    assert_eq!(initial_route.ready_retryable_count(), 383);
    assert_eq!(initial_route.delivering_count(), 0);
    assert_eq!(initial_route.next_turn_count(), 1);
    assert_eq!(initial_route.terminal_count(), 0);

    let mut gate_revision = initial_gate.revision();
    let mut route_revision = initial_route.revision();
    let mut ready_count = initial_route.ready_retryable_count();
    let mut delivering_count = initial_route.delivering_count();
    let mut terminal_count = initial_route.terminal_count();
    let mut steering_count = initial_gate.live_steering_count();
    let leaves = leaves
        .into_iter()
        .map(|leaf| {
            let begin_gate_revision = gate_revision;
            let begin_route_revision = route_revision;
            let delivering_revision = leaf.revision().checked_next().unwrap();
            let complete_gate_revision = begin_gate_revision.checked_next().unwrap();
            let complete_route_revision = begin_route_revision.checked_next().unwrap();

            ready_count -= 1;
            delivering_count += 1;
            gate_revision = complete_gate_revision;
            route_revision = complete_route_revision;

            let delivered_revision = delivering_revision.checked_next().unwrap();
            delivering_count -= 1;
            terminal_count += 1;
            steering_count -= 1;
            gate_revision = gate_revision.checked_next().unwrap();
            route_revision = route_revision.checked_next().unwrap();

            FixtureRecord::AcceptedRouteLeaf(fixture_route_leaf_with_transition(
                AcceptedRouteLeafRecord::new(
                    leaf.input_id(),
                    leaf.thread_id(),
                    leaf.generation(),
                    leaf.ordinal(),
                    delivered_revision,
                    AcceptedRouteLeafState::Routed,
                    AcceptedInputLifecycle::Delivered,
                ),
                AcceptedRouteLeafTransitionProof::new(
                    complete_gate_revision,
                    AcceptedRouteHeadProof::new(generation, complete_route_revision),
                    delivering_revision,
                    AcceptedRouteLeafTransitionKind::Complete,
                ),
            ))
        })
        .collect::<Vec<_>>();
    for chunk in leaves.chunks(96) {
        commit(store, storage.clone(), batch(chunk.iter().cloned()));
    }

    assert_eq!(ready_count, 84);
    assert_eq!(delivering_count, 0);
    assert_eq!(terminal_count, 299);
    assert_eq!(steering_count, 84);
    let final_head = AcceptedRouteHeadProof::new(generation, route_revision);
    commit(
        store,
        storage.clone(),
        batch([
            FixtureRecord::InputGate(
                InputGateRecord::new(
                    thread,
                    gate_revision,
                    initial_gate.state().clone(),
                    initial_gate.accepted_high_water(),
                    initial_gate.route_generation_high_water(),
                    Some(final_head),
                    steering_count,
                    initial_gate.live_next_turn_count(),
                    initial_gate.live_logical_utf8_bytes(),
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedRouteGeneration(
                AcceptedRouteGenerationRecord::new(
                    thread,
                    generation,
                    route_revision,
                    initial_route.target().clone(),
                    initial_route.first_ordinal(),
                    initial_route.last_ordinal(),
                    initial_route.input_count(),
                    ready_count,
                    delivering_count,
                    initial_route.next_turn_count(),
                    terminal_count,
                    initial_route.live_logical_utf8_bytes(),
                    initial_route.delivering_logical_utf8_bytes(),
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedRouteGenerationHead(AcceptedRouteGenerationHeadRecord::new(
                thread, final_head,
            )),
            FixtureRecord::AcceptedReadySource(AcceptedReadySourceRecord::new(
                thread,
                gate_revision,
                generation,
                route_revision,
                initial_route.first_ordinal().unwrap(),
                initial_route.last_ordinal().unwrap(),
            )),
            FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
                thread,
                generation,
                route_revision,
                initial_route.first_ordinal().unwrap(),
                initial_route.last_ordinal().unwrap(),
            )),
        ]),
    );
}

#[test]
fn candidate_pages_advance_across_terminal_history_and_preserve_accepted_order() {
    let home = TestHome::new("phase57-terminal-heavy-ready");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_terminal_heavy(&store, &storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let source = source(&store, &storage);
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
    let old_source = source(&store, &storage);
    let first = storage
        .accepted_ready_candidate_page(&store, old_source, None, limits(1))
        .unwrap();
    assert_eq!(first.records().len(), 1);
    let old_cursor = first.next_cursor().expect("source interval continues");

    match store.execute_current(AcceptedOperation::Retry.current_command(&storage)) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => {
            panic!("expected retry command to commit without later failure, got {outcome:?}")
        }
    }
    let new_source = source(&store, &storage);
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
    assert_eq!(source(&store, &storage).thread_id(), id(40));
    match store.execute_current(AcceptedOperation::Begin.current_command(&storage)) {
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
    seed_populated(&store, storage.clone());
    corruption(storage, &store);
    let error = store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
    store.close().unwrap();

    let mut reopened = open(home.path());
    let _storage = SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
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
