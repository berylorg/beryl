use beryl_home_store::HomeStore;
use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, SyndicAcceptedInputId, SyndicDraftId,
    ThreadRevision,
};
use syndic_storage::test_faults::{FixtureRecord, fixture_route_leaf_with_transition};
use syndic_storage::*;

use super::{
    composer_content_records, draft_id, id,
    populated::{
        active_snapshot, active_turn, cas_thread, cas_turn, next_input, populated_records,
    },
    timestamp,
};

pub const DELIVERY_UNKNOWN_LOGICAL_BYTES: u64 = 19;

pub fn delivering_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([70; 16])
}

pub fn retryable_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([71; 16])
}

pub fn mixed_abandonment_records() -> Vec<FixtureRecord> {
    let mut records = populated_records();
    let final_thread_revision = ThreadRevision::new(5).unwrap();
    let final_gate_revision = InputGateRevision::new(6).unwrap();
    let empty_content = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(input) if input.id() == next_input() => {
                Some(input.content())
            }
            _ => None,
        })
        .unwrap();
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("possibly dispatched").unwrap()]).unwrap();
    let (delivering_content, delivering_content_records) = composer_content_records(&payload);
    assert_eq!(
        delivering_content.summary().logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );
    for record in &mut records {
        if let FixtureRecord::Thread(thread) = record
            && thread.id() == id(40)
        {
            *thread = ThreadRecord::new(
                thread.id(),
                SelectedPathProof::new(
                    thread.committed_tail(),
                    final_thread_revision,
                    thread.selected_path_digest(),
                ),
                thread.current_draft_id(),
                thread.lineage(),
                thread.image_label_frontiers(),
                thread.context_owner_id(),
            );
        } else if let FixtureRecord::DraftByThread(index) = record
            && index.thread_id() == id(40)
        {
            *index = DraftByThreadRecord::new(
                index.thread_id(),
                index.draft_id(),
                index.draft_revision(),
                final_thread_revision,
            );
        } else if let FixtureRecord::HistorySummary(summary) = record
            && summary.thread_id() == id(40)
        {
            *summary = HistorySummaryRecord::new(
                summary.thread_id(),
                summary.revision().checked_next().unwrap(),
                final_thread_revision,
                summary.committed_tail(),
                summary.selected_path_digest(),
                summary.complete(),
                summary.last_activity_at(),
            );
        } else if let FixtureRecord::AcceptedInput(input) = record
            && input.id() == next_input()
        {
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
                    SyndicDraftId::from_bytes(*delivering_input().as_bytes()),
                )
                .unwrap(),
                input.route_generation(),
                input.content(),
                input.asset_reference_set(),
                input.admitted_at(),
            )
            .unwrap();
        } else if let FixtureRecord::InputGate(gate) = record
            && gate.thread_id() == id(40)
        {
            *gate = InputGateRecord::new(
                gate.thread_id(),
                final_gate_revision,
                gate.state().clone(),
                4,
                Some(AcceptedRouteGeneration::FIRST),
                Some(AcceptedRouteHeadProof::new(
                    AcceptedRouteGeneration::FIRST,
                    AcceptedRouteRevision::new(3).unwrap(),
                )),
                3,
                1,
                DELIVERY_UNKNOWN_LOGICAL_BYTES,
            )
            .unwrap();
        } else if let FixtureRecord::AcceptedRouteGeneration(route) = record
            && route.thread_id() == id(40)
        {
            *route = AcceptedRouteGenerationRecord::new(
                id(40),
                AcceptedRouteGeneration::FIRST,
                AcceptedRouteRevision::new(3).unwrap(),
                route.target().clone(),
                Some(AcceptedInputOrdinal::FIRST),
                Some(AcceptedInputOrdinal::new(4).unwrap()),
                4,
                2,
                1,
                1,
                0,
                DELIVERY_UNKNOWN_LOGICAL_BYTES,
                DELIVERY_UNKNOWN_LOGICAL_BYTES,
            )
            .unwrap();
        } else if let FixtureRecord::AcceptedRouteGenerationHead(head) = record
            && head.thread_id() == id(40)
        {
            *head = AcceptedRouteGenerationHeadRecord::new(
                id(40),
                AcceptedRouteHeadProof::new(
                    AcceptedRouteGeneration::FIRST,
                    AcceptedRouteRevision::new(3).unwrap(),
                ),
            );
        } else if let FixtureRecord::AcceptedReadySource(source) = record
            && source.thread_id() == id(40)
        {
            *source = AcceptedReadySourceRecord::new(
                id(40),
                final_gate_revision,
                AcceptedRouteGeneration::FIRST,
                AcceptedRouteRevision::new(3).unwrap(),
                AcceptedInputOrdinal::FIRST,
                AcceptedInputOrdinal::new(4).unwrap(),
            );
        } else if let FixtureRecord::AcceptedNextSource(source) = record
            && source.thread_id() == id(40)
        {
            *source = AcceptedNextSourceRecord::new(
                id(40),
                AcceptedRouteGeneration::FIRST,
                AcceptedRouteRevision::new(3).unwrap(),
                AcceptedInputOrdinal::FIRST,
                AcceptedInputOrdinal::new(4).unwrap(),
            );
        }
    }
    for (input_id, ordinal, lifecycle, content, replacement) in [
        (
            delivering_input(),
            3,
            AcceptedInputLifecycle::Delivering,
            delivering_content,
            SyndicDraftId::from_bytes(*retryable_input().as_bytes()),
        ),
        (
            retryable_input(),
            4,
            AcceptedInputLifecycle::Retryable,
            empty_content,
            draft_id(41),
        ),
    ] {
        let ordinal = AcceptedInputOrdinal::new(ordinal).unwrap();
        let revision = AcceptedInputRevision::new(2).unwrap();
        let (transition_gate, transition_route, transition_kind) = match lifecycle {
            AcceptedInputLifecycle::Delivering => (
                InputGateRevision::new(4).unwrap(),
                AcceptedRouteRevision::new(1).unwrap(),
                AcceptedRouteLeafTransitionKind::Begin,
            ),
            AcceptedInputLifecycle::Retryable => (
                InputGateRevision::new(5).unwrap(),
                AcceptedRouteRevision::new(2).unwrap(),
                AcceptedRouteLeafTransitionKind::Retry,
            ),
            _ => unreachable!(),
        };
        records.extend([
            FixtureRecord::AcceptedInput(
                AcceptedInputRecord::new(
                    input_id,
                    id(40),
                    ordinal,
                    AcceptedInputAdmissionProof::new(
                        ThreadRevision::new(ordinal.get()).unwrap(),
                        SyndicDraftId::from_bytes(*input_id.as_bytes()),
                        DraftRevision::new(1).unwrap(),
                        InputGateRevision::new(ordinal.get()).unwrap(),
                        replacement,
                    )
                    .unwrap(),
                    AcceptedRouteGeneration::FIRST,
                    content,
                    None,
                    timestamp(8),
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                id(40),
                ordinal,
                input_id,
                AcceptedRouteGeneration::FIRST,
            )),
            FixtureRecord::AcceptedRouteLeaf(fixture_route_leaf_with_transition(
                AcceptedRouteLeafRecord::new(
                    input_id,
                    id(40),
                    AcceptedRouteGeneration::FIRST,
                    ordinal,
                    revision,
                    AcceptedRouteLeafState::Routed,
                    lifecycle,
                ),
                AcceptedRouteLeafTransitionProof::new(
                    transition_gate,
                    AcceptedRouteHeadProof::new(AcceptedRouteGeneration::FIRST, transition_route),
                    AcceptedInputRevision::new(1).unwrap(),
                    transition_kind,
                ),
            )),
        ]);
    }
    records.extend(delivering_content_records);
    records
}

pub fn abandonment_request(store: &HomeStore, storage: SyndicStorage) -> AbandonActiveBinding {
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let binding = storage
        .current_binding(store, id(40), limit)
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("fixture binding is not active");
    };
    let snapshot = storage
        .execution_snapshot(store, active_snapshot(), limit)
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(store, id(40), limit).unwrap().unwrap();
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(snapshot.loaded_generation()),
        "active projection authority lost",
        timestamp(9),
    )
    .unwrap();
    AbandonActiveBinding::new(
        id(40),
        binding.binding().revision(),
        gate.selected_route().unwrap().generation(),
        AcceptedRouteLostTarget::Steering(SteeringTargetProof::new(
            PendingSteeringTargetProof::new(
                binding.binding().revision(),
                active_snapshot(),
                active_turn(),
                cas_thread(),
            ),
            cas_turn(),
        )),
        binding.binding().selected_path(),
        stale,
    )
}
