use beryl_home_store::HomeStore;

use crate::{
    CompactionOperationState, CompactionRequestDisposition, CompactionSettlement,
    PublishCompactionRequestDisposition, SyndicReadError, SyndicStorage,
};

use super::{CompactionRequestTransitionStatus, SyndicPointReadLimit};

impl SyndicStorage {
    /// Reconciles one compact-start response against live authority or its terminal successor.
    pub fn compaction_request_disposition_status(
        &self,
        store: &HomeStore,
        request: &PublishCompactionRequestDisposition,
        limit: SyndicPointReadLimit,
    ) -> Result<CompactionRequestTransitionStatus, SyndicReadError> {
        let Some(case) = self.compaction_recovery_read(store, request.operation_id(), limit)?
        else {
            return Ok(CompactionRequestTransitionStatus::Collision);
        };
        let operation = case.record();
        if operation.id() != request.operation_id()
            || operation.attempt() != request.attempt()
            || operation
                .dispatch_claim()
                .is_none_or(|claim| claim.attempt() != request.attempt())
        {
            return Ok(CompactionRequestTransitionStatus::Collision);
        }

        if let Some(observation) = operation.request() {
            let successor_revision = request
                .expected_operation_revision()
                .checked_next()
                .map_err(|_| {
                    SyndicReadError::Invariant(
                        "compaction request reconciliation revision frontier is exhausted",
                    )
                })?;
            return Ok(
                if observation.revision() == successor_revision
                    && observation.disposition() == request.disposition()
                {
                    CompactionRequestTransitionStatus::Exact
                } else {
                    CompactionRequestTransitionStatus::Collision
                },
            );
        }

        if operation.revision() == request.expected_operation_revision()
            && terminal_successor_is_compatible(operation, request.disposition())
        {
            let first = self.consumed_compaction_successor_is_exact(store, operation, limit)?;
            let second = self.consumed_compaction_successor_is_exact(store, operation, limit)?;
            if !first || !second {
                return Err(SyndicReadError::Invariant(
                    "consumed compaction settlement successor is not durably authenticated",
                ));
            }
            return Ok(CompactionRequestTransitionStatus::TerminalAlreadySettled);
        }

        Ok(
            if operation.revision() == request.expected_operation_revision()
                && live_state_admits(operation.state(), request.disposition())
            {
                CompactionRequestTransitionStatus::Prior
            } else {
                CompactionRequestTransitionStatus::Collision
            },
        )
    }
}

fn live_state_admits(
    state: &CompactionOperationState,
    disposition: CompactionRequestDisposition,
) -> bool {
    match state {
        CompactionOperationState::DispatchClaimed => true,
        CompactionOperationState::Finalizing => matches!(
            disposition,
            CompactionRequestDisposition::Accepted
                | CompactionRequestDisposition::CompletionUnknown
        ),
        _ => false,
    }
}

fn terminal_successor_is_compatible(
    operation: &crate::CompactionOperationRecord,
    disposition: CompactionRequestDisposition,
) -> bool {
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        return false;
    };
    operation.terminal().is_some()
        && matches!(
            witness.settlement(),
            CompactionSettlement::ManualSuccess
                | CompactionSettlement::ManualFailure
                | CompactionSettlement::LifecycleUserWorkWon
                | CompactionSettlement::LifecycleContinuation { .. }
        )
        && matches!(
            disposition,
            CompactionRequestDisposition::Accepted
                | CompactionRequestDisposition::CompletionUnknown
        )
}
