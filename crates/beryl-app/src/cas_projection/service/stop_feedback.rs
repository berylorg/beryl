use std::sync::{Arc, Mutex};

use super::commands::PreparedStop;
use crate::cas_projection::{
    ExactSoftStopAvailability, ExactSoftStopEligibility, ExactSoftStopUnavailable,
    ExactStopFeedback, ExactStopFeedbackState, ExactStopRequestError,
    stop::{EligibilityRecord, StopCoordinationError},
};

impl super::stop_worker::ExactStopRead {
    pub fn exact_soft_stop_eligibility(
        &self,
        thread: beryl_model::SyndicThreadId,
    ) -> ExactSoftStopAvailability {
        let epoch = match self.stop_coordinator.feedback_eligibility_epoch() {
            Ok(epoch) => epoch,
            Err(_) => {
                return ExactSoftStopAvailability::Unavailable(
                    ExactSoftStopUnavailable::AuthorityUnavailable,
                );
            }
        };
        match self.prepare_stop(thread) {
            Ok(PreparedStop::Exact { stopping: true, .. }) => {
                ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::RequestInProgress)
            }
            Ok(PreparedStop::Exact { target, proof, .. }) => {
                if self.stop_coordinator.has_waiting_feedback(&target) {
                    return ExactSoftStopAvailability::Unavailable(
                        ExactSoftStopUnavailable::RequestInProgress,
                    );
                }
                ExactSoftStopAvailability::Eligible(ExactSoftStopEligibility {
                    inner: Arc::new(EligibilityRecord {
                        owner: Arc::downgrade(&self.stop_coordinator),
                        target,
                        proof,
                        epoch,
                        consumed: Mutex::new(None),
                    }),
                })
            }
            Ok(PreparedStop::Ineligible(_)) => {
                ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::NoExactTarget)
            }
            Err(_) => ExactSoftStopAvailability::Unavailable(
                ExactSoftStopUnavailable::AuthorityUnavailable,
            ),
        }
    }

    pub fn request_exact_soft_stop(
        &self,
        eligibility: &ExactSoftStopEligibility,
    ) -> Result<ExactStopFeedback, ExactStopRequestError> {
        let token = &eligibility.inner;
        if !token.owner.ptr_eq(&Arc::downgrade(&self.stop_coordinator)) {
            return Err(ExactStopRequestError::Revoked);
        }
        let mut consumed = token
            .consumed
            .lock()
            .map_err(|_| ExactStopRequestError::Revoked)?;
        if let Some(record) = consumed.as_ref() {
            return record
                .upgrade()
                .map(|inner| ExactStopFeedback { inner })
                .ok_or(ExactStopRequestError::Revoked);
        }
        let (connection, proof) = match self.prepare_stop(token.target.thread_id()) {
            Ok(PreparedStop::Exact {
                target,
                connection,
                proof,
                ..
            }) if target == token.target && proof == token.proof => (connection, proof),
            _ => return Err(ExactStopRequestError::Revoked),
        };
        let (feedback, primary) =
            self.stop_coordinator
                .reserve_feedback(&token.target, &proof, token.epoch)?;
        *consumed = Some(Arc::downgrade(&feedback.inner));
        drop(consumed);
        if !primary {
            return Ok(feedback);
        }
        let result = self.coordinate_prepared_stop(
            connection,
            proof,
            syndic_storage::StopCause::SelectedOperationControl,
        );
        if let Err(error) = result {
            let snapshot = feedback.snapshot();
            if snapshot.attempt == crate::cas_projection::ExactStopAttemptKind::AdmissionPending {
                let state = match error {
                    StopCoordinationError::CommandCommitted { .. }
                    | StopCoordinationError::CommandIndeterminate { .. } => {
                        ExactStopFeedbackState::Waiting
                    }
                    _ => ExactStopFeedbackState::RequestNotAdmitted,
                };
                feedback.inner.update(state, None);
            }
        }
        Ok(feedback)
    }
}

impl super::ProjectionConnectionService {
    pub fn exact_soft_stop_eligibility(
        &self,
        thread: beryl_model::SyndicThreadId,
    ) -> ExactSoftStopAvailability {
        self.exact_stop_read().exact_soft_stop_eligibility(thread)
    }

    pub fn request_exact_soft_stop(
        &self,
        eligibility: &ExactSoftStopEligibility,
    ) -> Result<ExactStopFeedback, ExactStopRequestError> {
        self.exact_stop_read().request_exact_soft_stop(eligibility)
    }
}
