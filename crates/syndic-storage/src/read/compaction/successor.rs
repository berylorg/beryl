use beryl_home_store::HomeStore;

use crate::{
    BindingState, CompactionOperationRecord, CompactionOperationState, CompactionSettlement,
    ConversationParent, SyndicReadError, SyndicStorage, TurnKind, TurnLifecycle, codec::*,
};

use super::SyndicPointReadLimit;

impl SyndicStorage {
    pub(super) fn consumed_compaction_successor_is_exact(
        &self,
        store: &HomeStore,
        operation: &CompactionOperationRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let CompactionOperationState::Consumed(witness) = operation.state() else {
            return Ok(false);
        };
        let Some(receipt) =
            self.point::<CompactionSettlementReceiptsFamily>(store, operation.id(), limit)?
        else {
            return Ok(false);
        };
        let Some(gate) =
            self.point::<InputGatesFamily>(store, operation.target().thread_id(), limit)?
        else {
            return Ok(false);
        };
        let Some(state) =
            self.point::<TurnStatesFamily>(store, operation.target().turn_id(), limit)?
        else {
            return Ok(false);
        };
        if !operation.consumed_receipt_is_exact(&receipt)
            || !receipt.current_gate_is_descendant(&gate)
            || !provider_lifecycle_matches(operation, witness.settlement(), &state)
            || !self.stop_source_is_exact(store, operation, &receipt, limit)?
        {
            return Ok(false);
        }
        match witness.settlement() {
            CompactionSettlement::CancelledBeforeDispatch
            | CompactionSettlement::LocalNondispatch
            | CompactionSettlement::ManualSuccess => {
                self.preserved_binding_is_exact(store, operation, limit)
            }
            CompactionSettlement::Abandoned(_) => {
                self.retired_binding_is_exact(store, operation, limit)
            }
            CompactionSettlement::ManualFailure => {
                if manual_failure_preserves_binding(operation) {
                    self.preserved_binding_is_exact(store, operation, limit)
                } else {
                    self.retired_binding_is_exact(store, operation, limit)
                }
            }
            CompactionSettlement::LifecycleUserWorkWon => {
                if !self.preserved_binding_is_exact(store, operation, limit)? {
                    return Ok(false);
                }
                self.accepted_work_witness_is_exact(store, operation, &receipt, limit)
            }
            CompactionSettlement::LifecycleContinuation {
                turn_id,
                item_id,
                content_id,
            } => self.continuation_successor_is_exact(
                store,
                operation,
                &receipt,
                *turn_id,
                *item_id,
                *content_id,
                limit,
            ),
        }
    }

    fn stop_source_is_exact(
        &self,
        store: &HomeStore,
        operation: &CompactionOperationRecord,
        receipt: &crate::CompactionSettlementReceiptRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let crate::InputGateState::Stopping {
            operation_nonce, ..
        } = receipt.source_gate().state()
        else {
            return Ok(true);
        };
        let id = crate::StopOperationId::new(operation.target().thread_id(), *operation_nonce);
        Ok(self
            .point::<StopOperationsFamily>(store, id, limit)?
            .is_some_and(|stop| stop.provider_abandonment_authenticates(operation, receipt)))
    }

    fn preserved_binding_is_exact(
        &self,
        store: &HomeStore,
        operation: &CompactionOperationRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let target = operation.target();
        let binding = self.point::<BindingsFamily>(
            store,
            BindingKey {
                thread: target.thread_id(),
                revision: target.binding_revision(),
            },
            limit,
        )?;
        Ok(binding.is_some_and(|binding| {
            matches!(binding.state(), BindingState::Valid(usable)
                if usable.cas_thread_id() == target.cas_thread_id()
                    && usable.execution().runtime_id() == target.runtime_id())
        }))
    }

    fn retired_binding_is_exact(
        &self,
        store: &HomeStore,
        operation: &CompactionOperationRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let target = operation.target();
        let Ok(revision) = target.binding_revision().checked_next() else {
            return Ok(false);
        };
        let binding = self.point::<BindingsFamily>(
            store,
            BindingKey {
                thread: target.thread_id(),
                revision,
            },
            limit,
        )?;
        let reservation = self.point::<CasThreadIndexFamily>(
            store,
            CasThreadKey::Record(target.cas_thread_id().clone()),
            limit,
        )?;
        Ok(binding.is_some_and(|binding| {
            binding.thread_id() == target.thread_id()
                && matches!(binding.state(), BindingState::Stale(_))
        }) && reservation.is_some_and(|reservation| {
            reservation.thread_id() == target.thread_id()
                && reservation.retired_binding_revision() == Some(revision)
        }))
    }

    fn accepted_work_witness_is_exact(
        &self,
        store: &HomeStore,
        operation: &CompactionOperationRecord,
        receipt: &crate::CompactionSettlementReceiptRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let high_water = receipt.source_gate().accepted_high_water();
        let Ok(ordinal) = crate::AcceptedInputOrdinal::new(high_water) else {
            return Ok(false);
        };
        if receipt.source_gate().live_next_turn_count() == 0 {
            return Ok(false);
        }
        let Some(order) = self.point::<AcceptedOrderFamily>(
            store,
            ThreadAcceptedKey {
                owner: operation.target().thread_id(),
                ordinal,
            },
            limit,
        )?
        else {
            return Ok(false);
        };
        let input = self.point::<AcceptedInputsFamily>(store, order.input_id(), limit)?;
        Ok(input.is_some_and(|input| {
            order.thread_id() == operation.target().thread_id()
                && order.ordinal() == ordinal
                && input.thread_id() == operation.target().thread_id()
                && input.ordinal() == ordinal
                && input.route_generation() == order.route_generation()
                && input.admission_gate_revision() <= receipt.source_gate().revision()
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn continuation_successor_is_exact(
        &self,
        store: &HomeStore,
        operation: &CompactionOperationRecord,
        receipt: &crate::CompactionSettlementReceiptRecord,
        turn_id: beryl_model::SyndicTurnId,
        item_id: beryl_model::SyndicItemId,
        content_id: beryl_model::SyndicContentId,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let Some(continuation) = receipt.continuation() else {
            return Ok(false);
        };
        let prepared = crate::prepare_lifecycle_continuation_content().map_err(|_| {
            SyndicReadError::Invariant("fixed lifecycle-continuation content is invalid")
        })?;
        let expected_turn = crate::derive_lifecycle_continuation_turn_id(
            operation.home_id(),
            operation.id(),
            prepared.summary().digest(),
        );
        let expected_item = crate::derive_lifecycle_continuation_item_id(
            operation.home_id(),
            operation.id(),
            prepared.summary().digest(),
        );
        let Some(snapshot) =
            self.point::<ExecutionSnapshotsFamily>(store, operation.target().snapshot_id(), limit)?
        else {
            return Ok(false);
        };
        let Some(turn) = self.point::<TurnsFamily>(store, turn_id, limit)? else {
            return Ok(false);
        };
        let Some(state) = self.point::<TurnStatesFamily>(store, turn_id, limit)? else {
            return Ok(false);
        };
        let Some(item) = self.point::<CanonicalItemsFamily>(store, item_id, limit)? else {
            return Ok(false);
        };
        let Some(content) = self.point::<ContentManifestsFamily>(store, content_id, limit)? else {
            return Ok(false);
        };
        let binding = self.point::<BindingsFamily>(
            store,
            BindingKey {
                thread: operation.target().thread_id(),
                revision: continuation.binding_revision(),
            },
            limit,
        )?;
        let admission_path = snapshot.selected_path();
        let expected_parent = ConversationParent::from_turn(admission_path.tail());
        let Some((expected_depth, expected_digest, expected_ancestor_skip)) =
            self.continuation_turn_shape(store, turn_id, expected_parent, limit)?
        else {
            return Ok(false);
        };
        let Ok(thread_revision) = admission_path.thread_revision().checked_next() else {
            return Ok(false);
        };
        let expected_path =
            crate::SelectedPathProof::new(Some(turn_id), thread_revision, expected_digest);
        let Ok(binding_revision) = operation.target().binding_revision().checked_next() else {
            return Ok(false);
        };
        Ok(turn_id == expected_turn
            && item_id == expected_item
            && content_id == prepared.id()
            && continuation.parent() == expected_parent
            && continuation.selected_path() == expected_path
            && continuation.binding_revision() == binding_revision
            && continuation.content().id() == prepared.id()
            && continuation.content().encoding() == prepared.encoding()
            && continuation.content().summary() == prepared.summary()
            && turn.origin_thread_id() == operation.target().thread_id()
            && turn.kind() == TurnKind::BerylLifecycleContinuation
            && turn.parent() == expected_parent
            && turn.depth() == expected_depth
            && turn.chain_digest() == expected_digest
            && turn.ancestor_skip() == expected_ancestor_skip
            && continuation_lifecycle_is_descendant(&state)
            && binding.is_some_and(|binding| {
                binding.thread_id() == operation.target().thread_id()
                    && binding.selected_path() == expected_path
                    && matches!(binding.state(), BindingState::Unbound { .. })
            })
            && item.turn_id() == turn_id
            && item.ordinal() == crate::TurnItemOrdinal::FIRST
            && item.kind() == crate::CanonicalItemKind::UserInput
            && item.presentation_content() == Some(continuation.content())
            && item.presentation().asset_reference_set().is_none()
            && content.owner().is_none()
            && content.lifecycle() == crate::ContentLifecycle::Sealed
            && content.sealed_reference() == Some(continuation.content()))
    }

    fn continuation_turn_shape(
        &self,
        store: &HomeStore,
        turn_id: beryl_model::SyndicTurnId,
        parent: ConversationParent,
        limit: SyndicPointReadLimit,
    ) -> Result<
        Option<(
            crate::TurnDepth,
            beryl_model::SyndicPathDigest,
            Option<beryl_model::SyndicTurnId>,
        )>,
        SyndicReadError,
    > {
        match parent {
            ConversationParent::Root => Ok(Some((
                crate::TurnDepth::FIRST,
                crate::root_turn_chain_digest(turn_id),
                None,
            ))),
            ConversationParent::Turn(parent_id) => {
                let Some(parent) = self.point::<TurnsFamily>(store, parent_id, limit)? else {
                    return Ok(None);
                };
                let Ok(depth) = parent.depth().checked_next() else {
                    return Ok(None);
                };
                let ancestor_skip = crate::selected_path::child_ancestor_skip(
                    parent.clone(),
                    depth,
                    |ancestor_id| {
                        self.point::<TurnsFamily>(store, ancestor_id, limit)?.ok_or(
                            SyndicReadError::Invariant(
                                "compaction continuation admission ancestor is missing",
                            ),
                        )
                    },
                    SyndicReadError::Invariant,
                )?;
                Ok(Some((
                    depth,
                    crate::child_turn_chain_digest(turn_id, parent_id, parent.chain_digest()),
                    Some(ancestor_skip),
                )))
            }
        }
    }
}

fn provider_lifecycle_matches(
    operation: &CompactionOperationRecord,
    settlement: &CompactionSettlement,
    state: &crate::TurnStateRecord,
) -> bool {
    match settlement {
        CompactionSettlement::CancelledBeforeDispatch | CompactionSettlement::LocalNondispatch => {
            state.lifecycle() == TurnLifecycle::Failed
        }
        CompactionSettlement::Abandoned(_) => {
            state.lifecycle() == TurnLifecycle::Incomplete
                && state.incomplete_reason() == Some(crate::TurnIncompleteReason::AuthorityLost)
        }
        CompactionSettlement::ManualSuccess
        | CompactionSettlement::LifecycleUserWorkWon
        | CompactionSettlement::LifecycleContinuation { .. } => {
            state.lifecycle() == TurnLifecycle::Complete
        }
        CompactionSettlement::ManualFailure => operation.terminal().is_some_and(|terminal| {
            if terminal.status().outcome() == crate::TurnTerminalOutcome::Complete {
                state.lifecycle() == TurnLifecycle::Incomplete
                    && state.incomplete_reason()
                        == Some(crate::TurnIncompleteReason::CompletionMismatch)
            } else {
                state.end_status() == Some(terminal.status())
            }
        }),
    }
}

fn manual_failure_preserves_binding(operation: &CompactionOperationRecord) -> bool {
    operation.terminal().is_some_and(|terminal| {
        terminal.status().outcome() == crate::TurnTerminalOutcome::Interrupted
            && operation.status().is_some_and(|status| {
                status.status() == crate::CompactionThreadStatus::Idle
                    && status.sequence() < terminal.sequence()
            })
    })
}

fn continuation_lifecycle_is_descendant(state: &crate::TurnStateRecord) -> bool {
    if state.item_count() == 0 {
        return false;
    }
    match state.lifecycle() {
        TurnLifecycle::Pending => {
            state.revision() == crate::TurnStateRevision::FIRST
                && state.source_event_count() == 0
                && state.item_count() == 1
                && state.finalized_item_count() == 0
                && state.open_item_count() == 1
                && state.history_blocking_item_count() == 0
                && state.end_status().is_none()
        }
        TurnLifecycle::Active
        | TurnLifecycle::UnknownTerminal
        | TurnLifecycle::Complete
        | TurnLifecycle::Interrupted
        | TurnLifecycle::Failed
        | TurnLifecycle::Incomplete => {
            state.revision() > crate::TurnStateRevision::FIRST && state.source_event_count() > 0
        }
    }
}
