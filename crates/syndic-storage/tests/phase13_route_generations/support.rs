use beryl_home_store::HomeStore;
use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, SyndicAcceptedInputId, SyndicDraftId,
    ThreadRevision,
};
use syndic_storage::test_faults::{FixtureRecord, fixture_route_leaf_with_transition};
use syndic_storage::*;

use crate::support::{
    composer_content_records, draft_id, id,
    populated::{active_snapshot, active_turn, cas_thread, cas_turn, next_input, seed_populated},
    timestamp,
};

const DELIVERY_UNKNOWN_LOGICAL_BYTES: u64 = 19;

pub fn delivering_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([70; 16])
}

pub fn retryable_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([71; 16])
}

pub fn seed_mixed_abandonment(store: &HomeStore, storage: SyndicStorage) {
    seed_populated(store, storage);
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let thread = storage.thread(store, id(40), limit).unwrap().unwrap();
    let draft = storage
        .current_draft(store, id(40), limit)
        .unwrap()
        .unwrap();
    let summary = storage
        .history_summary(store, id(40), limit)
        .unwrap()
        .unwrap();
    let next = storage
        .accepted_input(store, next_input(), limit)
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(store, id(40), limit).unwrap().unwrap();
    let head = syndic_storage::test_faults::accepted_route_generation_head(store, storage, id(40))
        .unwrap()
        .unwrap();
    let route = syndic_storage::test_faults::accepted_route_generation(
        store,
        storage,
        id(40),
        AcceptedRouteGeneration::FIRST,
    )
    .unwrap();
    let final_thread_revision = ThreadRevision::new(5).unwrap();
    let final_gate_revision = InputGateRevision::new(6).unwrap();
    let empty_content = next.content();
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("possibly dispatched").unwrap()]).unwrap();
    let (delivering_content, delivering_content_records) = composer_content_records(&payload);
    assert_eq!(
        delivering_content.summary().logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );
    let mut records = vec![
        FixtureRecord::Thread(ThreadRecord::new(
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
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            id(40),
            draft.draft().id(),
            draft.draft().revision(),
            final_thread_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            id(40),
            summary.revision().checked_next().unwrap(),
            final_thread_revision,
            summary.committed_tail(),
            summary.selected_path_digest(),
            summary.complete(),
            summary.last_activity_at(),
        )),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                next.id(),
                next.thread_id(),
                next.ordinal(),
                AcceptedInputAdmissionProof::new(
                    next.admission().expected_thread_revision(),
                    next.admission().source_draft_id(),
                    next.admission().expected_draft_revision(),
                    next.admission().expected_gate_revision(),
                    SyndicDraftId::from_bytes(*delivering_input().as_bytes()),
                )
                .unwrap(),
                next.route_generation(),
                next.content(),
                next.asset_reference_set(),
                next.admitted_at(),
            )
            .unwrap(),
        ),
        FixtureRecord::InputGate(
            InputGateRecord::new(
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
            .unwrap(),
        ),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
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
            .unwrap(),
        ),
        FixtureRecord::AcceptedRouteGenerationHead(AcceptedRouteGenerationHeadRecord::new(
            id(40),
            AcceptedRouteHeadProof::new(
                head.proof().generation(),
                AcceptedRouteRevision::new(3).unwrap(),
            ),
        )),
        FixtureRecord::AcceptedReadySource(AcceptedReadySourceRecord::new(
            id(40),
            final_gate_revision,
            AcceptedRouteGeneration::FIRST,
            AcceptedRouteRevision::new(3).unwrap(),
            AcceptedInputOrdinal::FIRST,
            AcceptedInputOrdinal::new(4).unwrap(),
        )),
        FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
            id(40),
            AcceptedRouteGeneration::FIRST,
            AcceptedRouteRevision::new(3).unwrap(),
            AcceptedInputOrdinal::FIRST,
            AcceptedInputOrdinal::new(4).unwrap(),
        )),
    ];
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
    crate::support::commit(store, storage, crate::support::batch(records));
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
