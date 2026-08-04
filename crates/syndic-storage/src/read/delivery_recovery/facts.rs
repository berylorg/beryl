use beryl_home_store::HomeStore;
use beryl_model::SyndicThreadId;

use crate::{
    AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord, AcceptedRouteTarget,
    ActiveCasTurnRecord, BindingRecord, BindingState, CompactionOperationId,
    CompactionOperationRecord, ExecutionSnapshotRecord, HistorySummaryRecord, InputGateRecord,
    StopOperationId, StopOperationRecord, SyndicCurrentBinding, SyndicPointReadLimit,
    SyndicReadError, TurnRecord, TurnStateRecord,
    codec::{
        AcceptedRouteGenerationHeadsFamily, AcceptedRouteGenerationsFamily, BindingKey,
        BindingsFamily, CompactionOperationsFamily, StopOperationsFamily, ThreadRouteKey,
    },
    domain::SyndicStorage,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::read) struct RecoveryFacts {
    pub(in crate::read) gate: Option<InputGateRecord>,
    pub(in crate::read) turn: Option<TurnRecord>,
    pub(in crate::read) state: Option<TurnStateRecord>,
    pub(in crate::read) summary: Option<HistorySummaryRecord>,
    pub(in crate::read) binding: Option<SyndicCurrentBinding>,
    pub(in crate::read) route_head: Option<AcceptedRouteGenerationHeadRecord>,
    pub(in crate::read) route: Option<AcceptedRouteGenerationRecord>,
    pub(in crate::read) snapshot: Option<ExecutionSnapshotRecord>,
    pub(in crate::read) active_turn: Option<ActiveCasTurnRecord>,
    pub(in crate::read) prior_binding: Option<BindingRecord>,
    pub(in crate::read) stop: Option<StopOperationRecord>,
    pub(in crate::read) compaction: Option<CompactionOperationRecord>,
}

pub(in crate::read) fn read(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
) -> Result<RecoveryFacts, SyndicReadError> {
    let gate = storage.input_gate(store, thread_id, limit)?;
    let state = match gate
        .as_ref()
        .and_then(|gate| gate.state().blocking_turn_id())
    {
        Some(turn_id) => storage.turn_state(store, turn_id, limit)?,
        None => None,
    };
    let turn = match gate
        .as_ref()
        .and_then(|gate| gate.state().blocking_turn_id())
    {
        Some(turn_id) => storage.turn(store, turn_id, limit)?,
        None => None,
    };
    let summary = if gate.is_some() {
        storage.history_summary(store, thread_id, limit)?
    } else {
        None
    };
    let binding = if gate.is_some() {
        storage.current_binding(store, thread_id, limit)?
    } else {
        None
    };
    let (route_head, route) = match gate.as_ref().and_then(InputGateRecord::selected_route) {
        Some(proof) => (
            storage.point::<AcceptedRouteGenerationHeadsFamily>(store, thread_id, limit)?,
            storage.point::<AcceptedRouteGenerationsFamily>(
                store,
                ThreadRouteKey {
                    thread: thread_id,
                    generation: proof.generation(),
                },
                limit,
            )?,
        ),
        None => (None, None),
    };
    let active_snapshot = binding
        .as_ref()
        .and_then(|binding| match binding.binding().state() {
            BindingState::Active(active) => Some(active.snapshot_id()),
            BindingState::Unbound { .. } | BindingState::Valid(_) | BindingState::Stale(_) => None,
        });
    let lost_snapshot = route.as_ref().and_then(|route| match route.target() {
        AcceptedRouteTarget::ProjectionLost(proof) => Some(proof.snapshot_id()),
        AcceptedRouteTarget::AwaitingSteering(_)
        | AcceptedRouteTarget::Steering(_)
        | AcceptedRouteTarget::AwaitingTerminal(_)
        | AcceptedRouteTarget::NextTurn(_) => None,
    });
    let compaction = match gate.as_ref().and_then(|gate| {
        gate.state()
            .compaction_operation_nonce()
            .map(|nonce| CompactionOperationId::new(thread_id, nonce))
    }) {
        Some(id) => storage.point::<CompactionOperationsFamily>(store, id, limit)?,
        None => None,
    };
    let compaction_snapshot = compaction
        .as_ref()
        .map(|operation| operation.target().snapshot_id());
    let snapshot_id = active_snapshot.or(lost_snapshot).or(compaction_snapshot);
    let snapshot = match snapshot_id {
        Some(snapshot_id) => storage.execution_snapshot(store, snapshot_id, limit)?,
        None => None,
    };
    let active_turn = match snapshot_id {
        Some(snapshot_id) => storage.active_cas_turn(store, snapshot_id, limit)?,
        None => None,
    };
    let prior_binding = match route.as_ref().and_then(|route| match route.target() {
        AcceptedRouteTarget::ProjectionLost(proof) => {
            Some(proof.abandonment().expected_binding_revision())
        }
        AcceptedRouteTarget::AwaitingSteering(_)
        | AcceptedRouteTarget::Steering(_)
        | AcceptedRouteTarget::AwaitingTerminal(_)
        | AcceptedRouteTarget::NextTurn(_) => None,
    }) {
        Some(revision) => storage.point::<BindingsFamily>(
            store,
            BindingKey {
                thread: thread_id,
                revision,
            },
            limit,
        )?,
        None => None,
    };
    let stop = match gate.as_ref().and_then(|gate| {
        gate.state()
            .stop_operation_nonce()
            .map(|nonce| StopOperationId::new(thread_id, nonce))
    }) {
        Some(id) => storage.point::<StopOperationsFamily>(store, id, limit)?,
        None => None,
    };
    Ok(RecoveryFacts {
        gate,
        turn,
        state,
        summary,
        binding,
        route_head,
        route,
        snapshot,
        active_turn,
        prior_binding,
        stop,
        compaction,
    })
}
