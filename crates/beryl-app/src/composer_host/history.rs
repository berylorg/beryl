mod position;

use beryl_home_store::{CommandCancellation, CommandError, CommandOutcome, HomeCommand, HomeStore};
use gpui_text_input::{
    MutationKey, MutationKind, RangeHistoryCommit, RangeHistoryIntent, RangeHistoryOutcome,
    RangeSourceSelection,
};
use syndic_storage::{
    DraftHistoricalRootAdoptionOutcomeV1, DraftHistoricalRootAdoptionReconciliationV1,
    DraftHistoricalRootDirectionV1, DraftHistoricalRootSelectionIntentV1,
    DraftHistoricalRootSelectionV1, DraftPieceOperationIdV1, PreparedDraftHistoricalRootAdoptionV1,
};

use super::request::validate_store;
use super::{ComposerHostBinding, ComposerHostError, SyndicComposerHost};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostHistoryStatus {
    Admitted,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostRetainedHistoryIntent {
    binding: ComposerHostBinding,
    intent: RangeHistoryIntent,
}

impl ComposerHostRetainedHistoryIntent {
    pub const fn binding(self) -> ComposerHostBinding {
        self.binding
    }

    pub const fn intent(self) -> RangeHistoryIntent {
        self.intent
    }
}

pub(super) enum ComposerHostPendingHistory {
    Prepared(Box<ComposerHostHistoryCoordinator>),
    Terminal(Box<ComposerHostTerminalHistory>),
    Unavailable(Box<ComposerHostRetainedHistoryIntent>),
}

pub(super) struct ComposerHostTerminalHistory {
    key: MutationKey,
    outcome: RangeHistoryOutcome,
}

#[derive(Clone)]
pub(super) struct ComposerHostHistoryCoordinator {
    binding: ComposerHostBinding,
    intent: RangeHistoryIntent,
    prepared: PreparedDraftHistoricalRootAdoptionV1,
    detached: bool,
}

impl ComposerHostPendingHistory {
    fn key(&self) -> MutationKey {
        match self {
            Self::Prepared(pending) => pending.intent.key(),
            Self::Terminal(terminal) => terminal.key,
            Self::Unavailable(intent) => intent.intent.key(),
        }
    }

    fn retained_intent(&self) -> Option<&ComposerHostRetainedHistoryIntent> {
        match self {
            Self::Unavailable(intent) => Some(intent),
            _ => None,
        }
    }
}

impl SyndicComposerHost {
    pub fn history_status(&self) -> Option<ComposerHostHistoryStatus> {
        match self.pending_history {
            Some(ComposerHostPendingHistory::Unavailable(_)) => {
                Some(ComposerHostHistoryStatus::Unavailable)
            }
            Some(_) => Some(ComposerHostHistoryStatus::Admitted),
            None => None,
        }
    }

    pub fn retained_history_intent(&self) -> Option<&ComposerHostRetainedHistoryIntent> {
        self.pending_history
            .as_ref()
            .and_then(ComposerHostPendingHistory::retained_intent)
    }

    pub fn begin_history_selection(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        intent: RangeHistoryIntent,
    ) -> Result<(), ComposerHostError> {
        if self.lifecycle.freezes_admission() {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        if self.live_operation_pending() {
            return Err(if self.pending_history.is_some() {
                ComposerHostError::HistoryPending
            } else {
                ComposerHostError::MutationPending
            });
        }
        self.reserve_settlement_custody()?;
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != binding {
            return Err(ComposerHostError::OldBinding);
        }
        if active.unavailable || active.session_disposed {
            return Err(ComposerHostError::HistoryUnavailable);
        }
        validate_store(binding, store)?;
        validate_history_intent(binding, intent)?;
        if let Some((last_binding, last_intent)) = self.last_history_identity.as_deref()
            && *last_binding == binding
        {
            let operation = intent.key().operation().get();
            let last_operation = last_intent.key().operation().get();
            if operation < last_operation {
                return Err(ComposerHostError::StaleRequestIdentity);
            }
            if operation == last_operation && intent != *last_intent {
                return Err(ComposerHostError::MutationIdentityCollision);
            }
            if operation == last_operation {
                let outcome = self
                    .last_history_outcome
                    .as_deref()
                    .copied()
                    .ok_or(ComposerHostError::StaleRequestIdentity)?;
                self.pending_history = Some(ComposerHostPendingHistory::Terminal(Box::new(
                    ComposerHostTerminalHistory {
                        key: intent.key(),
                        outcome,
                    },
                )));
                return Ok(());
            }
        }
        self.last_history_identity = Some(Box::new((binding, intent)));
        self.last_history_outcome = None;

        let direction = match intent.kind() {
            MutationKind::Undo => DraftHistoricalRootDirectionV1::Undo,
            MutationKind::Redo => DraftHistoricalRootDirectionV1::Redo,
            MutationKind::Edit => return Err(ComposerHostError::HistoryMalformed),
        };
        let selection = self.storage.prepare_draft_historical_root_selection(
            store,
            DraftHistoricalRootSelectionIntentV1::new(
                binding.candidate(),
                operation_id(intent.key().operation().get()),
                direction,
            ),
        );
        self.pending_history = Some(match selection {
            Ok(DraftHistoricalRootSelectionV1::Prepared(prepared)) => {
                ComposerHostPendingHistory::Prepared(Box::new(ComposerHostHistoryCoordinator {
                    binding,
                    intent,
                    prepared,
                    detached: false,
                }))
            }
            Ok(DraftHistoricalRootSelectionV1::Unavailable) => {
                ComposerHostPendingHistory::Terminal(Box::new(ComposerHostTerminalHistory {
                    key: intent.key(),
                    outcome: RangeHistoryOutcome::Rejected,
                }))
            }
            Err(_) => ComposerHostPendingHistory::Terminal(Box::new(ComposerHostTerminalHistory {
                key: intent.key(),
                outcome: RangeHistoryOutcome::Error,
            })),
        });
        Ok(())
    }

    pub fn execute_history_selection(
        &mut self,
        store: &HomeStore,
        key: MutationKey,
        cancellation: &CommandCancellation,
    ) -> Result<RangeHistoryOutcome, ComposerHostError> {
        if self
            .detached_history
            .iter()
            .any(|pending| pending.key() == key)
        {
            let outcome = self.execute_detached_history_selection(store, key, cancellation)?;
            self.record_history_outcome(key, outcome);
            return Ok(outcome);
        }
        let pending = self
            .pending_history
            .take()
            .ok_or(ComposerHostError::HistoryNotPending)?;
        let result = self.execute_history_pending(store, key, cancellation, pending);
        match result {
            Ok(outcome) => {
                self.record_history_outcome(key, outcome);
                Ok(outcome)
            }
            Err((error, pending)) => {
                self.pending_history = Some(pending);
                Err(error)
            }
        }
    }

    fn record_history_outcome(&mut self, key: MutationKey, outcome: RangeHistoryOutcome) {
        if self
            .last_history_identity
            .as_deref()
            .is_some_and(|(_, intent)| intent.key() == key)
        {
            self.last_history_outcome = Some(Box::new(outcome));
        }
    }

    fn execute_detached_history_selection(
        &mut self,
        store: &HomeStore,
        key: MutationKey,
        cancellation: &CommandCancellation,
    ) -> Result<RangeHistoryOutcome, ComposerHostError> {
        let position = self
            .detached_history
            .iter()
            .position(|pending| pending.key() == key)
            .ok_or(ComposerHostError::HistoryNotPending)?;
        let pending = self.detached_history.remove(position);
        match self.execute_history_pending(store, key, cancellation, pending) {
            Ok(outcome) => Ok(outcome),
            Err((error, pending)) => {
                self.detached_history.push(pending);
                Err(error)
            }
        }
    }

    fn execute_history_pending(
        &mut self,
        store: &HomeStore,
        key: MutationKey,
        cancellation: &CommandCancellation,
        pending: ComposerHostPendingHistory,
    ) -> Result<RangeHistoryOutcome, (ComposerHostError, ComposerHostPendingHistory)> {
        match pending {
            ComposerHostPendingHistory::Terminal(terminal) => {
                if key != terminal.key {
                    return Err((
                        ComposerHostError::RequestMismatch,
                        ComposerHostPendingHistory::Terminal(terminal),
                    ));
                }
                Ok(terminal.outcome)
            }
            ComposerHostPendingHistory::Unavailable(intent) => Err((
                ComposerHostError::HistoryUnavailable,
                ComposerHostPendingHistory::Unavailable(intent),
            )),
            ComposerHostPendingHistory::Prepared(pending) => {
                self.execute_prepared_history(store, key, cancellation, pending)
            }
        }
    }

    fn execute_prepared_history(
        &mut self,
        store: &HomeStore,
        key: MutationKey,
        cancellation: &CommandCancellation,
        pending: Box<ComposerHostHistoryCoordinator>,
    ) -> Result<RangeHistoryOutcome, (ComposerHostError, ComposerHostPendingHistory)> {
        if key != pending.intent.key() {
            return Err((
                ComposerHostError::RequestMismatch,
                ComposerHostPendingHistory::Prepared(pending),
            ));
        }
        if let Err(error) = validate_store(pending.binding, store) {
            return Err((error, ComposerHostPendingHistory::Prepared(pending)));
        }
        if cancellation.is_cancelled() {
            return Ok(RangeHistoryOutcome::Cancelled);
        }
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.history_before_execute_fault.take() {
            fault(store, self.storage);
        }
        let mut command = HomeCommand::new(store.home_revision().map_err(|error| {
            (
                ComposerHostError::HomeRead(error),
                ComposerHostPendingHistory::Prepared(pending.clone()),
            )
        })?)
        .with_cancellation(cancellation.clone());
        command
            .add(self.storage.adopt_draft_historical_root(
                self.storage.revision(store).map_err(|error| {
                    (
                        ComposerHostError::HomeRead(error),
                        ComposerHostPendingHistory::Prepared(pending.clone()),
                    )
                })?,
                pending.prepared.clone(),
            ))
            .map_err(|error| {
                (
                    ComposerHostError::CommandBuild(error),
                    ComposerHostPendingHistory::Prepared(pending.clone()),
                )
            })?;
        let command_outcome = store.execute(command);
        let cancelled_before_admission = matches!(
            &command_outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::CancelledBeforeAdmission
            }
        );
        let reconciliation = self
            .storage
            .reconcile_draft_historical_root_adoption(store, &pending.prepared, command_outcome)
            .map_err(|error| {
                let intent = retained_intent(&pending);
                self.mark_active_unavailable(pending.binding);
                (
                    ComposerHostError::HistoryReconciliation(error),
                    ComposerHostPendingHistory::Unavailable(Box::new(intent)),
                )
            })?;
        match reconciliation {
            DraftHistoricalRootAdoptionReconciliationV1::ExactOld => {
                Ok(if cancelled_before_admission {
                    RangeHistoryOutcome::Cancelled
                } else {
                    RangeHistoryOutcome::Error
                })
            }
            DraftHistoricalRootAdoptionReconciliationV1::Collision => {
                let intent = retained_intent(&pending);
                self.mark_active_unavailable(pending.binding);
                Err((
                    ComposerHostError::HistoryUnavailable,
                    ComposerHostPendingHistory::Unavailable(Box::new(intent)),
                ))
            }
            DraftHistoricalRootAdoptionReconciliationV1::ExactNew(outcome) => {
                self.finish_history_outcome(store, pending, outcome)
            }
        }
    }

    fn finish_history_outcome(
        &mut self,
        store: &HomeStore,
        pending: Box<ComposerHostHistoryCoordinator>,
        outcome: DraftHistoricalRootAdoptionOutcomeV1,
    ) -> Result<RangeHistoryOutcome, (ComposerHostError, ComposerHostPendingHistory)> {
        let became_dirty = !self.is_dirty();
        let proof = match outcome {
            DraftHistoricalRootAdoptionOutcomeV1::Committed(proof) => proof,
            DraftHistoricalRootAdoptionOutcomeV1::Rejected(_) => {
                return Ok(RangeHistoryOutcome::Rejected);
            }
            DraftHistoricalRootAdoptionOutcomeV1::Conflict(_) => {
                return Ok(RangeHistoryOutcome::Conflict);
            }
            DraftHistoricalRootAdoptionOutcomeV1::Cancelled(_) => {
                return Ok(RangeHistoryOutcome::Cancelled);
            }
            DraftHistoricalRootAdoptionOutcomeV1::Error(_) => {
                return Ok(RangeHistoryOutcome::Error);
            }
        };
        let settlement = proof.settlement();
        let successor = settlement
            .successor_candidate()
            .ok_or_else(|| history_unavailable(&pending))?;
        let candidate =
            syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(successor);
        let binding = ComposerHostBinding::new(
            pending.binding.home_id(),
            pending.binding.home_generation(),
            pending.binding.host_generation(),
            candidate,
            pending.binding.presentation_generation(),
        );
        if !pending.detached {
            let active = self
                .active
                .as_mut()
                .ok_or_else(|| history_old_binding(&pending))?;
            if active.binding != pending.binding {
                return Err(history_old_binding(&pending));
            }
            active.binding = binding;
            active.storage_candidate = candidate;
            active.unavailable = true;
            self.pending.clear();
            self.last_request_id = 0;
        }
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.history_after_commit_fault.take() {
            fault(store, self.storage);
        }
        let request = settlement.request();
        let caret_request = request.caret();
        let selection_request = request.selection();
        let caret = self
            .history_position(store, binding, caret_request)
            .map_err(|_| history_unavailable_after_commit(&pending))?;
        let anchor = if selection_request == caret_request {
            caret
        } else {
            self.history_position(store, binding, selection_request)
                .map_err(|_| history_unavailable_after_commit(&pending))?
        };
        if !pending.detached
            && let Some(active) = self.active.as_mut()
            && active.binding == binding
        {
            active.unavailable = false;
            self.lifecycle.adopted(binding, became_dirty);
        }
        Ok(RangeHistoryOutcome::Committed(RangeHistoryCommit::new(
            binding.range_binding(),
            caret,
            RangeSourceSelection {
                anchor,
                head: caret,
            },
            binding.range_history_frontier(),
        )))
    }
}

fn validate_history_intent(
    binding: ComposerHostBinding,
    intent: RangeHistoryIntent,
) -> Result<(), ComposerHostError> {
    let range_binding = binding.range_binding();
    let frontier = binding.range_history_frontier();
    if intent.binding() != range_binding
        || intent.key().binding() != range_binding.binding()
        || intent.key().base_revision() != range_binding.revision()
        || intent.frontier() != frontier
        || intent.selection().head != intent.caret()
        || intent.caret().byte_offset.get() > range_binding.extent().byte_len()
        || intent.selection().anchor.byte_offset.get() > range_binding.extent().byte_len()
        || intent
            .selection()
            .anchor
            .compare_in_revision(intent.selection().head)
            .is_none()
    {
        return Err(ComposerHostError::HistoryMalformed);
    }
    match intent.kind() {
        MutationKind::Undo | MutationKind::Redo => Ok(()),
        MutationKind::Edit => Err(ComposerHostError::HistoryMalformed),
    }
}

fn operation_id(operation: u64) -> DraftPieceOperationIdV1 {
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&operation.to_be_bytes());
    DraftPieceOperationIdV1::from_bytes(bytes)
}

fn retained_intent(pending: &ComposerHostHistoryCoordinator) -> ComposerHostRetainedHistoryIntent {
    ComposerHostRetainedHistoryIntent {
        binding: pending.binding,
        intent: pending.intent,
    }
}

fn history_unavailable(
    pending: &ComposerHostHistoryCoordinator,
) -> (ComposerHostError, ComposerHostPendingHistory) {
    (
        ComposerHostError::HistoryUnavailable,
        ComposerHostPendingHistory::Unavailable(Box::new(retained_intent(pending))),
    )
}

fn history_unavailable_after_commit(
    pending: &ComposerHostHistoryCoordinator,
) -> (ComposerHostError, ComposerHostPendingHistory) {
    history_unavailable(pending)
}

fn history_old_binding(
    pending: &ComposerHostHistoryCoordinator,
) -> (ComposerHostError, ComposerHostPendingHistory) {
    (
        ComposerHostError::OldBinding,
        ComposerHostPendingHistory::Unavailable(Box::new(retained_intent(pending))),
    )
}
