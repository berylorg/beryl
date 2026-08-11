#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandOutcome, DomainRegistrationError, HomeCommand, HomeStore};
use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, SyndicAcceptedInputId, SyndicDraftId,
    ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use support::phase11::{
    abandonment_request, delivering_input, mixed_abandonment_records, retryable_input,
};
use support::populated::{next_input, populated_records, steering_input};
use support::*;

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed route-generation command, got {outcome:?}"),
    }
}

fn accepted_id(ordinal: u64) -> SyndicAcceptedInputId {
    let mut bytes = [0xD3; 16];
    bytes[..8].copy_from_slice(&ordinal.to_be_bytes());
    SyndicAcceptedInputId::from_bytes(bytes)
}

fn large_route_records(last_ordinal: u64) -> Vec<FixtureRecord> {
    assert!(last_ordinal > 256);
    let mut records = populated_records();
    let thread = id(40);
    let generation = AcceptedRouteGeneration::FIRST;
    let revision = AcceptedRouteRevision::new(2).unwrap();
    let final_thread_revision = ThreadRevision::new(last_ordinal + 1).unwrap();
    let final_gate_revision = InputGateRevision::new(last_ordinal + 1).unwrap();
    let content = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(input) if input.id() == next_input() => {
                Some(input.content())
            }
            _ => None,
        })
        .unwrap();
    for record in &mut records {
        match record {
            FixtureRecord::Thread(record) if record.id() == thread => {
                *record = ThreadRecord::new(
                    record.id(),
                    SelectedPathProof::new(
                        record.committed_tail(),
                        final_thread_revision,
                        record.selected_path_digest(),
                    ),
                    record.current_draft_id(),
                    record.lineage(),
                    record.image_label_frontiers(),
                    record.context_owner_id(),
                );
            }
            FixtureRecord::DraftByThread(index) if index.thread_id() == thread => {
                *index = DraftByThreadRecord::new(
                    index.thread_id(),
                    index.draft_id(),
                    index.draft_revision(),
                    final_thread_revision,
                );
            }
            FixtureRecord::HistorySummary(summary) if summary.thread_id() == thread => {
                *summary = HistorySummaryRecord::new(
                    summary.thread_id(),
                    summary.revision().checked_next().unwrap(),
                    final_thread_revision,
                    summary.committed_tail(),
                    summary.selected_path_digest(),
                    summary.complete(),
                    summary.last_activity_at(),
                );
            }
            FixtureRecord::AcceptedInput(input) if input.id() == next_input() => {
                let proof = input.admission();
                *input = AcceptedInputRecord::new(
                    input.id(),
                    input.thread_id(),
                    input.ordinal(),
                    AcceptedInputAdmissionProof::new(
                        proof.expected_thread_revision(),
                        proof.source_draft_id(),
                        proof.expected_draft_revision(),
                        proof.expected_gate_revision(),
                        SyndicDraftId::from_bytes(*accepted_id(3).as_bytes()),
                    )
                    .unwrap(),
                    input.route_generation(),
                    input.content(),
                    input.asset_reference_set(),
                    input.admitted_at(),
                )
                .unwrap();
            }
            FixtureRecord::InputGate(gate) if gate.thread_id() == thread => {
                *gate = InputGateRecord::new(
                    thread,
                    final_gate_revision,
                    gate.state().clone(),
                    last_ordinal,
                    Some(generation),
                    Some(AcceptedRouteHeadProof::new(generation, revision)),
                    last_ordinal - 1,
                    1,
                    0,
                )
                .unwrap();
            }
            FixtureRecord::AcceptedRouteGenerationHead(head) if head.thread_id() == thread => {
                *head = AcceptedRouteGenerationHeadRecord::new(
                    thread,
                    AcceptedRouteHeadProof::new(generation, revision),
                );
            }
            FixtureRecord::AcceptedRouteGeneration(route) if route.thread_id() == thread => {
                *route = AcceptedRouteGenerationRecord::new(
                    thread,
                    generation,
                    revision,
                    route.target().clone(),
                    Some(AcceptedInputOrdinal::FIRST),
                    Some(AcceptedInputOrdinal::new(last_ordinal).unwrap()),
                    last_ordinal,
                    last_ordinal - 1,
                    0,
                    1,
                    0,
                    0,
                    0,
                )
                .unwrap();
            }
            FixtureRecord::AcceptedReadySource(source) if source.thread_id() == thread => {
                *source = AcceptedReadySourceRecord::new(
                    thread,
                    final_gate_revision,
                    generation,
                    revision,
                    AcceptedInputOrdinal::FIRST,
                    AcceptedInputOrdinal::new(last_ordinal).unwrap(),
                );
            }
            FixtureRecord::AcceptedNextSource(source) if source.thread_id() == thread => {
                *source = AcceptedNextSourceRecord::new(
                    thread,
                    generation,
                    revision,
                    AcceptedInputOrdinal::FIRST,
                    AcceptedInputOrdinal::new(last_ordinal).unwrap(),
                );
            }
            _ => {}
        }
    }
    for value in 3..=last_ordinal {
        let ordinal = AcceptedInputOrdinal::new(value).unwrap();
        let input_id = accepted_id(value);
        records.extend([
            FixtureRecord::AcceptedInput(
                AcceptedInputRecord::new(
                    input_id,
                    thread,
                    ordinal,
                    AcceptedInputAdmissionProof::new(
                        ThreadRevision::new(value).unwrap(),
                        SyndicDraftId::from_bytes(*input_id.as_bytes()),
                        DraftRevision::new(1).unwrap(),
                        InputGateRevision::new(value).unwrap(),
                        if value == last_ordinal {
                            draft_id(41)
                        } else {
                            SyndicDraftId::from_bytes(*accepted_id(value + 1).as_bytes())
                        },
                    )
                    .unwrap(),
                    generation,
                    content,
                    None,
                    timestamp(8),
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                thread, ordinal, input_id, generation,
            )),
            FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
                input_id,
                thread,
                generation,
                ordinal,
                AcceptedInputRevision::new(1).unwrap(),
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Admitted,
            )),
        ]);
    }
    records
}

#[test]
fn route_generation_above_the_old_cap_pages_and_abandons_without_member_rewrites() {
    const INPUT_COUNT: u64 = 302;
    let home = TestHome::new("phase13-large-route-generation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(large_route_records(INPUT_COUNT)));

    let thread = id(40);
    let generation = AcceptedRouteGeneration::FIRST;
    let revision = AcceptedRouteRevision::new(2).unwrap();
    let mut cursor = None;
    let mut observed = 0_u64;
    loop {
        let page = storage
            .accepted_route_page(&store, thread, generation, revision, cursor)
            .unwrap();
        assert!(page.records().len() <= ACCEPTED_ROUTE_PAGE_MAX_RECORDS);
        assert!(page.stored_bytes() <= ACCEPTED_ROUTE_PAGE_MAX_STORED_BYTES);
        observed += u64::try_from(page.records().len()).unwrap();
        cursor = page.next_cursor();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(observed, INPUT_COUNT);

    execute(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            abandonment_request(&store, storage),
        ),
    );
    let gate = storage
        .input_gate(
            &store,
            thread,
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), INPUT_COUNT);
    let lost = gate.selected_route().unwrap();
    assert_eq!(lost.generation(), generation);
    assert_eq!(lost.revision(), AcceptedRouteRevision::new(3).unwrap());

    store.close().unwrap();
}

#[test]
fn route_pages_reject_stale_revisions_and_cross_revision_cursors() {
    let home = TestHome::new("phase13-route-page-revision");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(large_route_records(302)));

    let thread = id(40);
    let generation = AcceptedRouteGeneration::FIRST;
    let old_revision = AcceptedRouteRevision::new(2).unwrap();
    let cursor = storage
        .accepted_route_page(&store, thread, generation, old_revision, None)
        .unwrap()
        .next_cursor()
        .expect("large generation has another page");
    let target = storage
        .ready_steering_input(
            &store,
            steering_input(),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap()
        .target()
        .clone();
    execute(
        &store,
        storage.begin_accepted_input_delivery(
            storage.revision(&store).unwrap(),
            BeginAcceptedInputDelivery::new(
                thread,
                steering_input(),
                AcceptedInputRevision::new(1).unwrap(),
                target,
            ),
        ),
    );

    assert!(matches!(
        storage.accepted_route_page(&store, thread, generation, old_revision, None),
        Err(SyndicReadError::StaleAcceptedRoute)
    ));
    assert!(matches!(
        storage.accepted_route_page(
            &store,
            thread,
            generation,
            AcceptedRouteRevision::new(3).unwrap(),
            Some(cursor),
        ),
        Err(SyndicReadError::InvalidAcceptedRouteCursor)
    ));
    store.close().unwrap();
}

#[test]
fn projection_loss_resolves_mixed_leaves_from_one_compact_generation_transition() {
    let home = TestHome::new("phase13-mixed-route-abandonment");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(mixed_abandonment_records()));

    execute(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            abandonment_request(&store, storage),
        ),
    );
    let gate = storage
        .input_gate(
            &store,
            id(40),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), 3);
    assert_eq!(gate.live_logical_utf8_bytes(), 0);
    let proof = gate.selected_route().unwrap();
    let page = storage
        .accepted_route_page(&store, id(40), proof.generation(), proof.revision(), None)
        .unwrap();
    let state = |input_id| {
        page.records()
            .iter()
            .find(|entry| entry.input().id() == input_id)
            .unwrap()
            .effective_state()
    };
    assert_eq!(
        state(delivering_input()),
        AcceptedRouteEffectiveState::DeliveryUnknown
    );
    assert_eq!(
        state(retryable_input()),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(
        state(next_input()),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::SteeringRejected)
    );
    assert_eq!(
        state(steering_input()),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );

    store.close().unwrap();
}

#[test]
fn route_generation_rejects_checked_aggregate_overflow() {
    let result = AcceptedRouteGenerationRecord::new(
        id(40),
        AcceptedRouteGeneration::FIRST,
        AcceptedRouteRevision::FIRST,
        AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
        Some(AcceptedInputOrdinal::FIRST),
        Some(AcceptedInputOrdinal::new(u64::MAX).unwrap()),
        u64::MAX,
        u64::MAX,
        1,
        0,
        0,
        0,
        0,
    );
    assert!(matches!(
        result,
        Err(SyndicRecordError::LengthOverflow {
            kind: "accepted-route classified count"
        })
    ));
}

#[test]
fn reopen_rejects_route_generation_high_water_drift() {
    let home = TestHome::new("phase13-route-generation-high-water-drift");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let gate = storage
        .input_gate(
            &store,
            id(40),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    let corrupt = InputGateRecord::new(
        gate.thread_id(),
        gate.revision(),
        gate.state().clone(),
        gate.accepted_high_water(),
        Some(AcceptedRouteGeneration::new(2).unwrap()),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    commit(&store, storage, batch([FixtureRecord::InputGate(corrupt)]));
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("route-generation high-water drift reopened successfully"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        DomainRegistrationError::Validation { source, .. }
            if source.to_string() == "input-gate route-generation high-water disagrees"
    ));
    reopened.close().unwrap();
}

#[test]
fn reopen_rejects_a_gap_in_monotonic_route_generations() {
    let home = TestHome::new("phase13-route-generation-gap");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let gate = storage
        .input_gate(
            &store,
            id(40),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    let generation = AcceptedRouteGeneration::new(3).unwrap();
    let corrupt_gate = InputGateRecord::new(
        gate.thread_id(),
        gate.revision(),
        gate.state().clone(),
        gate.accepted_high_water(),
        Some(generation),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    let skipped = AcceptedRouteGenerationRecord::new(
        id(40),
        generation,
        AcceptedRouteRevision::FIRST,
        AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
        None,
        None,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    )
    .unwrap();
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::InputGate(corrupt_gate),
            FixtureRecord::AcceptedRouteGeneration(skipped),
        ]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("route-generation gap reopened successfully"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        DomainRegistrationError::Validation { source, .. }
            if source.to_string() == "accepted-route generations are not sequential"
    ));
    reopened.close().unwrap();
}
