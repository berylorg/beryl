use super::{access::Access, *};
use beryl_backend::{NonIdempotentRequestOutcome, TurnStartOutcome};
use beryl_state::{
    BranchHandoffJobRecord, HandoffFailureEvidence, HandoffFailureKind, HandoffJobTransition,
};
use syndic_storage::{BindingPublicationStatus, CancelBindingActivation, SyndicPointReadLimit};

#[derive(Clone)]
pub struct DiscussionParentNondispatch {
    pub(super) request: CancelBindingActivation,
    kind: HandoffFailureKind,
}

impl DiscussionParentNondispatch {
    pub(crate) fn from_start_outcome(
        request: CancelBindingActivation,
        outcome: Result<&TurnStartOutcome, crate::process_admission::ProcessAdmissionError>,
    ) -> Result<Self, DiscussionSettlementError> {
        let kind = match outcome {
            Err(_) => HandoffFailureKind::RuntimeUnavailable,
            Ok(NonIdempotentRequestOutcome::ExactRejection { .. }) => {
                HandoffFailureKind::CasRejectedBeforeAcceptance
            }
            Ok(NonIdempotentRequestOutcome::ProvenNotDispatched { .. }) => {
                HandoffFailureKind::TransientDeliveryFailure
            }
            Ok(
                NonIdempotentRequestOutcome::ExactResponse { .. }
                | NonIdempotentRequestOutcome::CompletionUnknown { .. },
            ) => return Err(DiscussionSettlementError::IdentityMismatch),
        };
        Ok(Self { request, kind })
    }

    #[cfg(feature = "test-faults")]
    pub fn for_test(request: CancelBindingActivation, kind: HandoffFailureKind) -> Self {
        assert!(matches!(
            kind,
            HandoffFailureKind::RuntimeUnavailable
                | HandoffFailureKind::CasRejectedBeforeAcceptance
                | HandoffFailureKind::TransientDeliveryFailure
        ));
        Self { request, kind }
    }

    #[cfg(feature = "test-faults")]
    pub fn from_start_outcome_for_test(
        request: CancelBindingActivation,
        outcome: Result<&TurnStartOutcome, crate::process_admission::ProcessAdmissionError>,
    ) -> Result<Self, DiscussionSettlementError> {
        Self::from_start_outcome(request, outcome)
    }
}

pub(super) fn prepare(
    access: Access<'_>,
    syndic: &SyndicStorage,
    job: &BranchHandoffJobRecord,
    evidence: DiscussionParentNondispatch,
    command: &mut HomeCommand,
) -> Result<
    (
        HandoffJobTransition,
        Option<SyndicSettlementIntent>,
        DiscussionSettlementResult,
    ),
    DiscussionSettlementError,
> {
    let parent = job
        .state()
        .parent()
        .ok_or(DiscussionSettlementError::IdentityMismatch)?;
    if evidence.request.thread_id() != job.parent_thread_id()
        || evidence.request.turn_id() != parent.turn_id()
    {
        return Err(DiscussionSettlementError::IdentityMismatch);
    }
    super::candidate::validate_job_sources(access, syndic, job)?;
    let limit = SyndicPointReadLimit::new(400_000).expect("bounded cancellation evidence");
    let (status, revision) = match access {
        Access::Ordinary(store) => (
            syndic.cancelled_binding_activation_status(store, &evidence.request, limit)?,
            syndic.revision(store)?,
        ),
        Access::Candidate(candidate) => (
            syndic.cancelled_binding_activation_status_candidate(
                candidate,
                &evidence.request,
                limit,
            )?,
            syndic.revision_candidate(candidate)?,
        ),
    };
    if status != BindingPublicationStatus::Prior {
        return Err(DiscussionSettlementError::IdentityMismatch);
    }
    command.add(syndic.cancel_binding_activation(revision, evidence.request.clone()))?;
    Ok((
        HandoffJobTransition::RetryableFailure(
            HandoffFailureEvidence::new(evidence.kind, None)
                .expect("empty failure detail is bounded"),
        ),
        Some(SyndicSettlementIntent::Cancellation(evidence.request)),
        DiscussionSettlementResult::ParentRetryable {
            parent,
            kind: evidence.kind,
        },
    ))
}
