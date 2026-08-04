use beryl_home_store::HomeStore;

use crate::{
    AcceptedNextSourceRecord, AcceptedReadySourceRecord, AcceptedRouteGenerationHeadRecord,
    AcceptedRouteGenerationRecord, ActiveCasTurnRecord, BindingHeadRecord, BindingRecord,
    CasThreadBindingIndexRecord, CasThreadIndexRecord, CasTurnIndexRecord, ExecutionSnapshotRecord,
    InputGateRecord, LiveSourceEvent, SourceEventRecord, StopOperationId, StopOperationRecord,
    StopOperationTarget, SyndicReadError, SyndicStorage, ThreadRecord, TurnRecord, TurnStateRecord,
    codec::*,
};

use super::super::SyndicPointReadLimit;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::read) struct StopObservation {
    pub(in crate::read) stop: Option<StopOperationRecord>,
    pub(in crate::read) gate: Option<InputGateRecord>,
    pub(in crate::read) route_head: Option<AcceptedRouteGenerationHeadRecord>,
    pub(in crate::read) route: Option<AcceptedRouteGenerationRecord>,
    pub(in crate::read) admission_route: Option<AcceptedRouteGenerationRecord>,
    pub(in crate::read) ready_source: Option<AcceptedReadySourceRecord>,
    pub(in crate::read) next_source: Option<AcceptedNextSourceRecord>,
    pub(in crate::read) thread: Option<ThreadRecord>,
    pub(in crate::read) binding_head: Option<BindingHeadRecord>,
    pub(in crate::read) binding: Option<BindingRecord>,
    pub(in crate::read) successor_binding: Option<BindingRecord>,
    pub(in crate::read) reservation: Option<CasThreadIndexRecord>,
    pub(in crate::read) membership: Option<CasThreadBindingIndexRecord>,
    pub(in crate::read) successor_membership: Option<CasThreadBindingIndexRecord>,
    pub(in crate::read) cas_turn: Option<CasTurnIndexRecord>,
    pub(in crate::read) snapshot: Option<ExecutionSnapshotRecord>,
    pub(in crate::read) active_turn: Option<ActiveCasTurnRecord>,
    pub(in crate::read) turn: Option<TurnRecord>,
    pub(in crate::read) turn_state: Option<TurnStateRecord>,
    pub(in crate::read) latest_event: Option<SourceEventRecord>,
    pub(in crate::read) compaction: Option<crate::CompactionOperationRecord>,
    pub(in crate::read) compaction_receipt: Option<crate::CompactionSettlementReceiptRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StopTerminalObservation {
    pub(super) stop: StopObservation,
    pub(super) event: Option<SourceEventRecord>,
}

impl SyndicStorage {
    pub(super) fn stable_stop_observation(
        &self,
        store: &HomeStore,
        operation_id: StopOperationId,
        target: &StopOperationTarget,
        limit: SyndicPointReadLimit,
        operation: &'static str,
    ) -> Result<StopObservation, SyndicReadError> {
        let first = StopObservation::read(self, store, operation_id, target, limit)?;
        let second = StopObservation::read(self, store, operation_id, target, limit)?;
        if first == second {
            Ok(first)
        } else {
            Err(SyndicReadError::ConcurrentChange { operation })
        }
    }

    pub(super) fn stable_stop_terminal_observation(
        &self,
        store: &HomeStore,
        operation_id: StopOperationId,
        target: &StopOperationTarget,
        event: &LiveSourceEvent,
        limit: SyndicPointReadLimit,
    ) -> Result<StopTerminalObservation, SyndicReadError> {
        let read = || -> Result<StopTerminalObservation, SyndicReadError> {
            Ok(StopTerminalObservation {
                stop: StopObservation::read(self, store, operation_id, target, limit)?,
                event: self.source_event(store, event.turn_id(), event.sequence(), limit)?,
            })
        };
        let first = read()?;
        let second = read()?;
        if first == second {
            Ok(first)
        } else {
            Err(SyndicReadError::ConcurrentChange {
                operation: "stop matching-terminal reconciliation",
            })
        }
    }
}

impl StopObservation {
    pub(in crate::read) fn read_current_stop(
        storage: &SyndicStorage,
        store: &HomeStore,
        operation_id: StopOperationId,
        target: &StopOperationTarget,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, SyndicReadError> {
        Self::read(storage, store, operation_id, target, limit)
    }

    pub(in crate::read) fn read_current_target(
        storage: &SyndicStorage,
        store: &HomeStore,
        thread_id: beryl_model::SyndicThreadId,
        target: &StopOperationTarget,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, SyndicReadError> {
        Self::read_keyed(storage, store, thread_id, None, target, limit)
    }

    fn read(
        storage: &SyndicStorage,
        store: &HomeStore,
        operation_id: StopOperationId,
        target: &StopOperationTarget,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, SyndicReadError> {
        Self::read_keyed(
            storage,
            store,
            operation_id.thread_id(),
            Some(operation_id),
            target,
            limit,
        )
    }

    fn read_keyed(
        storage: &SyndicStorage,
        store: &HomeStore,
        thread_id: beryl_model::SyndicThreadId,
        operation_id: Option<StopOperationId>,
        target: &StopOperationTarget,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, SyndicReadError> {
        let stop = operation_id
            .map(|id| storage.point::<StopOperationsFamily>(store, id, limit))
            .transpose()?
            .flatten();
        let gate = storage.point::<InputGatesFamily>(store, thread_id, limit)?;
        let route_head =
            storage.point::<AcceptedRouteGenerationHeadsFamily>(store, thread_id, limit)?;
        let route = gate
            .as_ref()
            .and_then(InputGateRecord::selected_route)
            .map(|proof| {
                storage.point::<AcceptedRouteGenerationsFamily>(
                    store,
                    ThreadRouteKey {
                        thread: thread_id,
                        generation: proof.generation(),
                    },
                    limit,
                )
            })
            .transpose()?
            .flatten();
        let admission_route = stop
            .as_ref()
            .and_then(|record| record.admission().successor_stopped_route_option())
            .map(|proof| {
                storage.point::<AcceptedRouteGenerationsFamily>(
                    store,
                    ThreadRouteKey {
                        thread: thread_id,
                        generation: proof.generation(),
                    },
                    limit,
                )
            })
            .transpose()?
            .flatten();
        let selected_route_key =
            gate.as_ref()
                .and_then(InputGateRecord::selected_route)
                .map(|proof| ThreadRouteKey {
                    thread: thread_id,
                    generation: proof.generation(),
                });
        let ready_source = selected_route_key
            .map(|key| storage.point::<AcceptedReadySourcesFamily>(store, key, limit))
            .transpose()?
            .flatten();
        let next_source = selected_route_key
            .map(|key| storage.point::<AcceptedNextSourcesFamily>(store, key, limit))
            .transpose()?
            .flatten();
        let successor_revision = stop
            .as_ref()
            .and_then(|_| target.binding_revision().checked_next().ok());
        let turn_state = storage.point::<TurnStatesFamily>(store, target.turn_id(), limit)?;
        let latest_event = turn_state
            .as_ref()
            .filter(|state| state.source_event_count() > 0)
            .and_then(|state| crate::SourceEventSequence::new(state.source_event_count()).ok())
            .map(|sequence| {
                storage.point::<SourceEventsFamily>(
                    store,
                    TurnEventKey {
                        owner: target.turn_id(),
                        ordinal: sequence,
                    },
                    limit,
                )
            })
            .transpose()?
            .flatten();
        let compaction_id = crate::CompactionOperationId::new(
            target.thread_id(),
            crate::CompactionOperationNonce::from_bytes(*target.turn_id().as_bytes()),
        );
        Ok(Self {
            stop,
            gate,
            route_head,
            route,
            admission_route,
            ready_source,
            next_source,
            thread: storage.point::<ThreadsFamily>(store, thread_id, limit)?,
            binding_head: storage.point::<BindingHeadsFamily>(store, thread_id, limit)?,
            binding: storage.point::<BindingsFamily>(
                store,
                BindingKey {
                    thread: thread_id,
                    revision: target.binding_revision(),
                },
                limit,
            )?,
            successor_binding: successor_revision
                .map(|revision| {
                    storage.point::<BindingsFamily>(
                        store,
                        BindingKey {
                            thread: thread_id,
                            revision,
                        },
                        limit,
                    )
                })
                .transpose()?
                .flatten(),
            reservation: storage.point::<CasThreadIndexFamily>(
                store,
                CasThreadKey::Record(target.cas_thread_id().clone()),
                limit,
            )?,
            membership: storage.point::<CasThreadBindingIndexFamily>(
                store,
                CasThreadBindingKey::Record(
                    target.cas_thread_id().clone(),
                    target.binding_revision(),
                ),
                limit,
            )?,
            successor_membership: successor_revision
                .map(|revision| {
                    storage.point::<CasThreadBindingIndexFamily>(
                        store,
                        CasThreadBindingKey::Record(target.cas_thread_id().clone(), revision),
                        limit,
                    )
                })
                .transpose()?
                .flatten(),
            cas_turn: storage.point::<CasTurnIndexFamily>(
                store,
                CasTurnKey::Record(target.cas_thread_id().clone(), target.cas_turn_id().clone()),
                limit,
            )?,
            snapshot: storage.point::<ExecutionSnapshotsFamily>(
                store,
                target.snapshot_id(),
                limit,
            )?,
            active_turn: storage.point::<ActiveCasTurnsFamily>(
                store,
                target.snapshot_id(),
                limit,
            )?,
            turn: storage.point::<TurnsFamily>(store, target.turn_id(), limit)?,
            turn_state,
            latest_event,
            compaction: storage.point::<CompactionOperationsFamily>(store, compaction_id, limit)?,
            compaction_receipt: storage.point::<CompactionSettlementReceiptsFamily>(
                store,
                compaction_id,
                limit,
            )?,
        })
    }
}
