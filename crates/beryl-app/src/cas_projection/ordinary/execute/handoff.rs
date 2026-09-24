use super::*;
use crate::cas_projection::ordinary::{
    OrdinaryTurnExecutionError, preflight::PendingOrdinaryExecution,
};
use crate::discussion_settlement::{
    DiscussionParentDispatchReservation, DiscussionParentNondispatch, DiscussionSettlementError,
    DiscussionSettlementOutcome, DiscussionSettlementService,
};
use beryl_home_store::{CommandCancellation, CommandError};
use beryl_model::JobId;
use syndic_storage::CanonicalItemPresentation;

pub(super) fn reserve(
    store: &HomeStore,
    storage: &SyndicStorage,
    pending: &PendingOrdinaryExecution,
    service: Option<&DiscussionSettlementService>,
) -> Result<Option<DiscussionParentDispatchReservation>, OrdinaryTurnExecutionError> {
    let item = storage
        .canonical_item(
            store,
            pending.item_id,
            crate::cas_projection::input_replay::point_limit(),
        )?
        .ok_or(OrdinaryTurnExecutionError::PendingTurnUnavailable {
            thread_id: pending.thread_id,
        })?;
    let CanonicalItemPresentation::DiscussionHandoff {
        accepted_input_id, ..
    } = item.presentation()
    else {
        return Ok(None);
    };
    let service = service
        .filter(|service| service.matches_home(store))
        .ok_or(OrdinaryTurnExecutionError::HandoffAuthorityUnavailable)?;
    Ok(Some(service.reserve_parent_dispatch(
        JobId::from_bytes(*accepted_input_id.as_bytes()),
        pending.thread_id,
        pending.turn_id,
    )?))
}

pub(super) fn settle(
    reservation: DiscussionParentDispatchReservation,
    evidence: DiscussionParentNondispatch,
    cancellation: &ProjectionCancellationToken,
) -> Result<(), OrdinaryTurnExecutionError> {
    let mut pending = reservation.nondispatched(evidence)?;
    loop {
        crate::cas_projection::input_replay::check_cancelled(cancellation)?;
        let prepared = match pending.prepare(CommandCancellation::new()) {
            Ok(prepared) => prepared,
            Err(DiscussionSettlementError::ConcurrentChange)
            | Err(DiscussionSettlementError::Syndic(
                syndic_storage::SyndicReadError::ConcurrentChange { .. },
            )) => {
                std::thread::yield_now();
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        match prepared.execute() {
            DiscussionSettlementOutcome::Committed {
                later_failure: None,
                ..
            } => return Ok(()),
            DiscussionSettlementOutcome::Committed {
                receipt,
                later_failure: Some(later_failure),
                ..
            } => {
                return Err(OrdinaryTurnExecutionError::HomeCommandCommitted {
                    receipt,
                    later_failure,
                });
            }
            DiscussionSettlementOutcome::Indeterminate { failure, .. } => {
                return Err(OrdinaryTurnExecutionError::HomeCommandIndeterminate { failure });
            }
            DiscussionSettlementOutcome::NotCommitted {
                evidence: DiscussionSettlementError::Command(CommandError::Conflict { .. }),
            } => std::thread::yield_now(),
            DiscussionSettlementOutcome::NotCommitted { evidence } => return Err(evidence.into()),
        }
    }
}
