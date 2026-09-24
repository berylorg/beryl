use super::{access::Access, *};
use beryl_backend::{NonIdempotentRequestOutcome, TurnStartOutcome};
use beryl_state::{
    BranchHandoffJobRecord, HandoffFailureEvidence, HandoffFailureKind, HandoffJobTransition,
};
use syndic_storage::{BindingPublicationStatus, CancelBindingActivation, SyndicPointReadLimit};

mod preactivation;
pub(crate) use preactivation::DiscussionPreparationFailure;

#[derive(Clone)]
pub struct DiscussionParentNondispatch {
    source: Source,
    kind: HandoffFailureKind,
}

#[derive(Clone)]
enum Source {
    Activated(CancelBindingActivation),
    BeforeActivation(preactivation::BeforeActivation),
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
        Ok(Self {
            source: Source::Activated(request),
            kind,
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn for_test(request: CancelBindingActivation, kind: HandoffFailureKind) -> Self {
        assert!(matches!(
            kind,
            HandoffFailureKind::RuntimeUnavailable
                | HandoffFailureKind::CasRejectedBeforeAcceptance
                | HandoffFailureKind::TransientDeliveryFailure
        ));
        Self {
            source: Source::Activated(request),
            kind,
        }
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
    if evidence.thread_id() != job.parent_thread_id() || evidence.turn_id() != parent.turn_id() {
        return Err(DiscussionSettlementError::IdentityMismatch);
    }
    super::candidate::validate_job_sources(access, syndic, job)?;
    let intent = match evidence.source {
        Source::BeforeActivation(proof) => {
            proof.validate(access, syndic)?;
            None
        }
        Source::Activated(request) => {
            let limit = SyndicPointReadLimit::new(400_000).expect("bounded cancellation evidence");
            let (status, revision) = match access {
                Access::Ordinary(store) => (
                    syndic.cancelled_binding_activation_status(store, &request, limit)?,
                    syndic.revision(store)?,
                ),
                Access::Candidate(candidate) => (
                    syndic.cancelled_binding_activation_status_candidate(
                        candidate, &request, limit,
                    )?,
                    syndic.revision_candidate(candidate)?,
                ),
            };
            if status != BindingPublicationStatus::Prior {
                return Err(DiscussionSettlementError::IdentityMismatch);
            }
            command.add(syndic.cancel_binding_activation(revision, request.clone()))?;
            Some(SyndicSettlementIntent::Cancellation(request))
        }
    };
    Ok((
        HandoffJobTransition::RetryableFailure(
            HandoffFailureEvidence::new(evidence.kind, None)
                .expect("empty failure detail is bounded"),
        ),
        intent,
        DiscussionSettlementResult::ParentRetryable {
            parent,
            kind: evidence.kind,
        },
    ))
}
