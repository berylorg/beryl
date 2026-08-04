#![cfg(feature = "test-faults")]

mod support;

use beryl_model::{
    AcceptedInputRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration,
    CasProcessGeneration, CasTurnId, DraftRevision, InputGateRevision, RootId, RuntimeId,
    SyndicAcceptedInputId, SyndicDraftId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureRecord, delivering_steering_read_metrics, reset_delivering_steering_read_metrics,
};
use syndic_storage::*;

use support::phase11::{
    DELIVERY_UNKNOWN_LOGICAL_BYTES, delivering_input, mixed_abandonment_records, retryable_input,
};
use support::populated::{
    active_snapshot, active_turn, cas_thread, cas_turn, populated_records, steering_input,
};
use support::*;

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn accepted_id(ordinal: u64) -> SyndicAcceptedInputId {
    let mut bytes = [0x52; 16];
    bytes[..8].copy_from_slice(&ordinal.to_be_bytes());
    SyndicAcceptedInputId::from_bytes(bytes)
}

fn large_delivering_generation(last_ordinal: u64) -> Vec<FixtureRecord> {
    assert!(last_ordinal > 256);
    let mut records = mixed_abandonment_records();
    let thread = id(40);
    let generation = AcceptedRouteGeneration::FIRST;
    let revision = AcceptedRouteRevision::new(3).unwrap();
    let final_thread_revision = ThreadRevision::new(last_ordinal + 1).unwrap();
    let final_gate_revision = InputGateRevision::new(last_ordinal + 1).unwrap();
    let empty_content = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(input) if input.id() == retryable_input() => {
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
            FixtureRecord::AcceptedInput(input) if input.id() == retryable_input() => {
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
                        SyndicDraftId::from_bytes(*accepted_id(5).as_bytes()),
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
                    DELIVERY_UNKNOWN_LOGICAL_BYTES,
                )
                .unwrap();
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
                    last_ordinal - 2,
                    1,
                    1,
                    0,
                    DELIVERY_UNKNOWN_LOGICAL_BYTES,
                    DELIVERY_UNKNOWN_LOGICAL_BYTES,
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

    for value in 5..=last_ordinal {
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
                    empty_content,
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
fn exact_delivering_input_resolves_with_fixed_point_work_on_a_large_generation() {
    let home = TestHome::new("phase52-exact-delivering-steering");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(large_delivering_generation(384)));

    reset_delivering_steering_read_metrics();
    let resolved = storage
        .delivering_steering_input(&store, delivering_input(), limit())
        .unwrap()
        .expect("fixture input is exactly delivering");

    assert_eq!(resolved.input().id(), delivering_input());
    assert_eq!(
        resolved.accepted_input_revision(),
        AcceptedInputRevision::new(2).unwrap()
    );
    assert_eq!(
        resolved.gate_revision(),
        InputGateRevision::new(385).unwrap()
    );
    assert_eq!(
        resolved.route(),
        AcceptedRouteHeadProof::new(
            AcceptedRouteGeneration::FIRST,
            AcceptedRouteRevision::new(3).unwrap(),
        )
    );
    assert_eq!(
        resolved.target().pending().binding_revision(),
        beryl_model::BindingRevision::new(3).unwrap()
    );
    assert_eq!(resolved.target().pending().snapshot_id(), active_snapshot());
    assert_eq!(resolved.target().pending().active_turn_id(), active_turn());
    assert_eq!(resolved.target().pending().cas_thread_id(), &cas_thread());
    assert_eq!(resolved.target().cas_turn_id(), &cas_turn());
    assert_eq!(
        resolved.execution().runtime_id(),
        RuntimeId::from_bytes([48; 16])
    );
    assert_eq!(resolved.execution().root_id(), RootId::from_bytes([49; 16]));
    assert_eq!(
        resolved.loaded_generation(),
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(1).unwrap(),
            CasLoadedThreadGeneration::new(1).unwrap(),
        )
    );
    assert_eq!(
        delivering_steering_read_metrics().point_reads(),
        12,
        "the exact read must not scale with route-generation membership",
    );

    store.close().unwrap();
}

#[test]
fn missing_and_non_delivering_inputs_are_not_eligible() {
    let home = TestHome::new("phase52-ineligible-delivering-steering");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));

    assert!(
        storage
            .delivering_steering_input(&store, steering_input(), limit())
            .unwrap()
            .is_none(),
        "a routed but merely admitted input is not a delayed delivering lifecycle",
    );
    assert!(
        storage
            .delivering_steering_input(
                &store,
                SyndicAcceptedInputId::from_bytes([0xEE; 16]),
                limit(),
            )
            .unwrap()
            .is_none(),
    );

    store.close().unwrap();
}

#[test]
fn inconsistent_active_cas_turn_relationship_is_an_invariant_failure() {
    let home = TestHome::new("phase52-corrupt-delivering-steering");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut records = mixed_abandonment_records();
    let publication = records
        .iter_mut()
        .find_map(|record| match record {
            FixtureRecord::ActiveCasTurn(publication)
                if publication.snapshot_id() == active_snapshot() =>
            {
                Some(publication)
            }
            _ => None,
        })
        .unwrap();
    *publication = ActiveCasTurnRecord::new(
        publication.snapshot_id(),
        publication.thread_id(),
        publication.turn_id(),
        publication.binding_revision(),
        publication.cas_thread_id().clone(),
        CasTurnId::new("wrong-delayed-steering-turn").unwrap(),
        publication.published_at(),
    );
    commit(&store, storage, batch(records));

    assert!(matches!(
        storage.delivering_steering_input(&store, delivering_input(), limit()),
        Err(SyndicReadError::Invariant(
            "delivering steering execution relationships disagree"
        ))
    ));

    store.close().unwrap();
}
