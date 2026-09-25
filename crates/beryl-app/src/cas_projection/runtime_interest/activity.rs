use super::*;
use beryl_home_store::{CommandOutcome, HomeStore};
use beryl_model::SyndicTurnId;
use syndic_storage::{
    ActivityEnrollmentPreparation, ActivityEnrollmentRequest, ActivityEnrollmentStatus,
    ActivityEnrollmentWitness, ActivityPeriodToken, ActivityQuerySource,
    ActivityRetirementFingerprint, ActivitySourceQualification, SyndicPointReadLimit,
    SyndicStorage,
};

use crate::cas_projection::{OrdinaryTurnExecutionError, ProjectionCancellationToken};
use crate::runtime_activity_enrollment::{
    ActivityEnrollmentAttempt, ActivityEnrollmentCommandOutcome,
};

#[derive(Default)]
pub(super) struct RuntimeActivityState {
    token: Option<ActivityPeriodToken>,
    pending: Option<ActivityEnrollmentAttempt>,
    committed: Option<ActivityEnrollmentWitness>,
}

impl RuntimeInterest {
    pub(in crate::cas_projection) fn with_activity<T>(
        &self,
        source: ActivityQuerySource,
        publish: impl FnOnce(ActivitySourceQualification) -> T,
    ) -> Option<T> {
        let state = self.shared.lock();
        let activity = self.activity.lock().unwrap_or_else(|e| e.into_inner());
        let token = activity.token.as_ref()?;
        let qualification = if matches!(self.status_locked(&state), RuntimeInterestStatus::Ready(_))
        {
            ActivitySourceQualification::Current {
                token: token.clone(),
                source,
            }
        } else {
            ActivitySourceQualification::Retired(ActivityRetirementFingerprint::from_enrollment(
                token, source,
            ))
        };
        Some(publish(qualification))
    }

    pub(in crate::cas_projection) fn enroll_activity(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        let source = ActivityQuerySource::new(thread, turn);
        let limit = SyndicPointReadLimit::new(65_536).expect("bounded enrollment point");
        loop {
            super::super::input_replay::check_cancelled(cancellation)?;
            let state = self.shared.lock();
            if self.kind != RuntimeInterestKind::RequiredWork
                || !matches!(self.status_locked(&state), RuntimeInterestStatus::Ready(_))
            {
                return Err(OrdinaryTurnExecutionError::ProjectionExecution(
                    super::super::ProjectionExecutionError::RuntimeInterestUnavailable,
                ));
            }
            let mut activity = self.activity.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(witness) = activity.committed.as_ref() {
                match storage.activity_enrollment_status(home, witness)? {
                    ActivityEnrollmentStatus::Committed { token } => {
                        if let Some(token) = token {
                            activity.token = Some(token);
                        }
                        activity.committed = None;
                    }
                    _ => {
                        return Err(OrdinaryTurnExecutionError::Invariant(
                            "committed enrollment proof changed",
                        ));
                    }
                }
            }
            if let Some(original) = activity.pending.as_ref() {
                let result = self
                    .shared
                    .enrollments
                    .settle_attempt(home, storage, original)?;
                activity.pending = None;
                match result {
                    ActivityEnrollmentStatus::Committed { token: Some(token) } => {
                        activity.token = Some(token)
                    }
                    ActivityEnrollmentStatus::Committed { token: None }
                    | ActivityEnrollmentStatus::NotCommitted => {}
                    ActivityEnrollmentStatus::Conflict => {
                        return Err(OrdinaryTurnExecutionError::Invariant(
                            "enrollment reconciliation conflict",
                        ));
                    }
                }
            } else {
                self.shared
                    .enrollments
                    .settle_retired_runtime(home, storage, self.runtime_id)?;
            }
            let head = storage.activity_query_head(home, thread, limit)?.ok_or(
                OrdinaryTurnExecutionError::Invariant("Activity head missing"),
            )?;
            let mut request = match activity.token.as_ref() {
                Some(token) => ActivityEnrollmentRequest::reuse(
                    source,
                    self.binding.clone(),
                    head.revision(),
                    token,
                ),
                None => {
                    ActivityEnrollmentRequest::first(source, self.binding.clone(), head.revision())
                }
            };
            if head.source() == Some(source)
                && activity
                    .token
                    .as_ref()
                    .is_none_or(|token| token.work_period() != head.work_period())
            {
                let fingerprint = storage
                    .activity_retirement_fingerprint(home, source)?
                    .ok_or(OrdinaryTurnExecutionError::Invariant(
                        "retired Activity head changed",
                    ))?;
                request = request.replace_retired_pending(fingerprint);
            }
            let prepared = match storage.prepare_activity_enrollment(home, request)? {
                ActivityEnrollmentPreparation::AlreadyEnrolled(token) => {
                    activity.token = Some(token);
                    return Ok(());
                }
                ActivityEnrollmentPreparation::Prepared(prepared) => prepared,
            };
            let reserved = self.shared.enrollments.reserve(home, prepared)?;
            activity.pending = Some(reserved.attempt());
            match reserved.execute() {
                ActivityEnrollmentCommandOutcome::Pending { failure } => {
                    return Err(OrdinaryTurnExecutionError::HomeCommandIndeterminate { failure });
                }
                ActivityEnrollmentCommandOutcome::Definitive { outcome, witness } => {
                    activity.pending = None;
                    match outcome {
                        CommandOutcome::NotCommitted { evidence } => {
                            return Err(OrdinaryTurnExecutionError::HomeCommandNotCommitted(
                                evidence,
                            ));
                        }
                        CommandOutcome::Committed {
                            later_failure: Some(later_failure),
                            receipt,
                            ..
                        } => {
                            activity.committed = Some(witness);
                            return Err(OrdinaryTurnExecutionError::HomeCommandCommitted {
                                receipt,
                                later_failure,
                            });
                        }
                        CommandOutcome::Committed {
                            later_failure: None,
                            ..
                        } => {
                            activity.committed = Some(witness);
                        }
                        CommandOutcome::Indeterminate { .. } => {
                            unreachable!("custody retains indeterminate outcomes")
                        }
                    }
                }
            }
        }
    }
}
