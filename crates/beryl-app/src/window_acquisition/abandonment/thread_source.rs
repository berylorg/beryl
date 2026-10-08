use super::*;
use syndic_storage::{
    EligibleEmptyThreadCandidate, EligibleEmptyThreadOutcome, EligibleEmptyThreadOutcomeAudit,
    SyndicReadError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum AcquiredThreadSource {
    Reused {
        outcome: EligibleEmptyThreadOutcome,
        candidate: EligibleEmptyThreadCandidate,
    },
    Created(PristineThreadCandidate),
}

impl AcquiredThreadSource {
    pub(super) fn origin(&self) -> WindowAcquisitionThreadOrigin {
        match self {
            Self::Reused { .. } => WindowAcquisitionThreadOrigin::Reused,
            Self::Created(_) => WindowAcquisitionThreadOrigin::CreatedFallback,
        }
    }
    pub(super) fn inspect(
        service: &RuntimeBackedWindowAcquisitionService,
        fingerprint: &AbandonmentFingerprint,
    ) -> Result<Option<Self>, SyndicReadError> {
        match fingerprint.disposition {
            RuntimeBackedWindowAcquisitionDisposition::Reused => {
                let Some(outcome) = fingerprint.reused_source.as_ref() else {
                    return Ok(None);
                };
                match service
                    .syndic
                    .audit_eligible_empty_thread_outcome(&service.store, outcome)?
                {
                    EligibleEmptyThreadOutcomeAudit::Exact(candidate)
                        if candidate.thread_id() == fingerprint.thread_id
                            && candidate.draft_id() == fingerprint.draft_id =>
                    {
                        Ok(Some(Self::Reused {
                            outcome: outcome.clone(),
                            candidate,
                        }))
                    }
                    _ => Ok(None),
                }
            }
            RuntimeBackedWindowAcquisitionDisposition::Created => {
                match service.syndic.audit_pristine_thread(
                    &service.store,
                    fingerprint.thread_id,
                    &fingerprint.fallback_execution,
                )? {
                    PristineThreadAudit::Exact(candidate)
                        if candidate_matches_fingerprint(&candidate, fingerprint) =>
                    {
                        Ok(Some(Self::Created(candidate)))
                    }
                    _ => Ok(None),
                }
            }
        }
    }
}
