use super::*;
use crate::discussion_settlement::{
    DiscussionParentDispatchReservation, DiscussionPreparationFailure, DiscussionSettlementError,
    DiscussionSettlementOutcome, DiscussionSettlementService,
};
use beryl_home_store::{CommandCancellation, CommandError};
use beryl_model::JobId;
use syndic_storage::{CanonicalItemPresentation, TurnKind};

pub(super) fn reserve(
    context: &PreparationContext,
    admission: &ScheduledOrdinaryAdmission,
    service: Option<&DiscussionSettlementService>,
) -> Result<Option<DiscussionParentDispatchReservation>, DiscussionSettlementError> {
    let limit = crate::cas_projection::input_replay::point_limit();
    let Some(pending) =
        context
            .storage
            .pending_dispatch_evidence(&context.home, admission.thread_id(), limit)?
    else {
        return Ok(None);
    };
    if pending.turn_kind() != TurnKind::BerylDiscussionHandoff {
        return Ok(None);
    }
    let item = context
        .storage
        .canonical_item(&context.home, pending.item_id(), limit)?
        .ok_or(DiscussionSettlementError::IdentityMismatch)?;
    let CanonicalItemPresentation::DiscussionHandoff {
        accepted_input_id, ..
    } = item.presentation()
    else {
        return Err(DiscussionSettlementError::IdentityMismatch);
    };
    let service = service
        .filter(|service| service.matches_home(&context.home))
        .ok_or(DiscussionSettlementError::ForeignHome)?;
    let reservation = service.reserve_parent_preparation(
        JobId::from_bytes(*accepted_input_id.as_bytes()),
        pending.thread_id(),
        pending.turn_id(),
    )?;
    if reservation
        .as_ref()
        .is_some_and(|reserved| reserved.execution_binding() != admission.execution_binding())
    {
        return Err(DiscussionSettlementError::IdentityMismatch);
    }
    Ok(reservation)
}

pub(super) fn settle(
    context: &PreparationContext,
    sessions: &ScheduledExecutionSessions,
    reservation: DiscussionParentDispatchReservation,
    failure: DiscussionPreparationFailure,
) {
    let Ok(mut proof) = reservation.preparation_failed(failure) else {
        return;
    };
    loop {
        if !context.commands.is_open() || sessions.lock().closed {
            return;
        }
        let prepared = match proof.prepare(CommandCancellation::new()) {
            Ok(prepared) => prepared,
            Err(DiscussionSettlementError::ConcurrentChange)
            | Err(DiscussionSettlementError::Syndic(
                syndic_storage::SyndicReadError::ConcurrentChange { .. },
            )) => {
                thread::yield_now();
                continue;
            }
            Err(_) => return,
        };
        match prepared.execute() {
            DiscussionSettlementOutcome::NotCommitted {
                evidence: DiscussionSettlementError::Command(CommandError::Conflict { .. }),
            } => thread::yield_now(),
            _ => return,
        }
    }
}

pub(super) fn runtime_failure(
    failure: crate::cas_projection::RuntimeFailure,
) -> DiscussionPreparationFailure {
    use crate::cas_projection::RuntimeFailure;
    match failure {
        RuntimeFailure::Admission
        | RuntimeFailure::ConnectionLost
        | RuntimeFailure::ProcessExited => DiscussionPreparationFailure::Cas,
        _ => DiscussionPreparationFailure::Runtime,
    }
}
