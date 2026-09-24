use super::{access::Access, *};
use crate::discussion_handoff_limits::HandoffScanLimits;
use beryl_state::{BranchHandoffJobLifecycle, BranchHandoffJobRecord};
use syndic_storage::{
    DiscussionHandoffGateState, DiscussionParentExecution, DiscussionParentExecutionDisposition,
    DiscussionParentExecutionRequest,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HandoffCandidateConvergenceSummary {
    pub job_visits: u64,
    pub transitions: u64,
}

#[derive(thiserror::Error)]
pub enum HandoffCandidateConvergenceError {
    #[error(transparent)]
    Read(#[from] beryl_home_store::ReadError),
    #[error(transparent)]
    LivePage(#[from] beryl_state::DurableJobReadError),
    #[error(transparent)]
    Settlement(#[from] DiscussionSettlementError),
    #[error("handoff convergence was cancelled")]
    Cancelled,
    #[error("handoff convergence slot configuration disagrees with operation custody")]
    SlotConfigurationMismatch,
    #[error("handoff convergence encountered contradictory job {job_id}")]
    IdentityMismatch { job_id: JobId },
    #[error("handoff convergence stopped with retained command outcome for job {job_id}")]
    Outcome {
        job_id: JobId,
        outcome: Box<DiscussionSettlementOutcome>,
    },
    #[error("handoff convergence progress counter exhausted")]
    CounterExhausted,
}
impl std::fmt::Debug for HandoffCandidateConvergenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, formatter)
    }
}

impl DiscussionSettlementOperations {
    pub fn converge_candidate(
        &self,
        candidate: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
        syndic: &SyndicStorage,
        limits: HandoffScanLimits,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
    ) -> Result<HandoffCandidateConvergenceSummary, HandoffCandidateConvergenceError> {
        if self.configured_slots() != limits.reconcile_slots() {
            return Err(HandoffCandidateConvergenceError::SlotConfigurationMismatch);
        }
        let access = Access::Candidate(candidate);
        let mut summary = HandoffCandidateConvergenceSummary::default();
        let mut after = None;
        let mut rescan = false;
        'scan: loop {
            check_cancelled(&cancellation)?;
            let revision = candidate.home_revision()?;
            let page = state
                .durable_jobs()
                .list_live_candidate(candidate, after, limits.page())?;
            if candidate.home_revision()? != revision {
                rescan = true;
                continue;
            }
            for expected in page.records() {
                check_cancelled(&cancellation)?;
                if candidate.home_revision()? != revision {
                    rescan = true;
                    continue 'scan;
                }
                let transitions = self.converge_one(
                    candidate,
                    state,
                    syndic,
                    expected,
                    at,
                    cancellation.clone(),
                )?;
                after = Some(expected.job_id());
                increment(&mut summary.job_visits, 1)?;
                increment(&mut summary.transitions, transitions)?;
                if transitions != 0 {
                    rescan = true;
                    continue 'scan;
                }
                if access.revision()? != revision {
                    rescan = true;
                    continue 'scan;
                }
            }
            if !page.has_more() {
                if rescan {
                    after = None;
                    rescan = false;
                    continue;
                }
                return Ok(summary);
            }
        }
    }

    fn converge_one(
        &self,
        candidate: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
        syndic: &SyndicStorage,
        expected: &BranchHandoffJobRecord,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
    ) -> Result<u64, HandoffCandidateConvergenceError> {
        let access = Access::Candidate(candidate);
        let mut job = access.job(state, expected.job_id())?;
        if &job != expected {
            return Err(HandoffCandidateConvergenceError::IdentityMismatch {
                job_id: expected.job_id(),
            });
        }
        let mut transitions = 0;
        for _ in 0..3 {
            check_cancelled(&cancellation)?;
            if !job.lifecycle().is_live() {
                return Ok(transitions);
            }
            let prepared = match job.lifecycle() {
                BranchHandoffJobLifecycle::WaitingResolvingTurn
                | BranchHandoffJobLifecycle::WaitingParent => self.prepare_candidate(
                    candidate,
                    state,
                    syndic,
                    job.job_id(),
                    cancellation.clone(),
                )?,
                BranchHandoffJobLifecycle::StartingParent
                | BranchHandoffJobLifecycle::ParentActive => self
                    .prepare_parent_execution_candidate(
                        candidate,
                        state,
                        syndic,
                        job.job_id(),
                        at,
                        cancellation.clone(),
                    )?,
                BranchHandoffJobLifecycle::RetryableFailed => {
                    validate_retryable(access, syndic, &job)?;
                    return Ok(transitions);
                }
                _ => {
                    return Err(HandoffCandidateConvergenceError::IdentityMismatch {
                        job_id: job.job_id(),
                    });
                }
            };
            let Some(prepared) = prepared else {
                return Ok(transitions);
            };
            match prepared.execute() {
                DiscussionSettlementOutcome::Committed {
                    later_failure: None,
                    local_finalization: None,
                    ..
                } => increment(&mut transitions, 1)?,
                outcome => {
                    return Err(HandoffCandidateConvergenceError::Outcome {
                        job_id: job.job_id(),
                        outcome: Box::new(outcome),
                    });
                }
            }
            job = access.job(state, job.job_id())?;
        }
        Err(HandoffCandidateConvergenceError::IdentityMismatch {
            job_id: job.job_id(),
        })
    }
}

pub(super) fn validate_retryable(
    access: Access<'_>,
    syndic: &SyndicStorage,
    job: &BranchHandoffJobRecord,
) -> Result<(), DiscussionSettlementError> {
    let before = access.revision()?;
    let gate = access.gate(syndic, job.discussion_thread_id())?;
    if gate.thread_id() != job.discussion_thread_id()
        || gate.state()
            != (DiscussionHandoffGateState::Pending {
                intent_id: job.intent_id(),
                job_id: job.job_id(),
                resolving_turn_id: job.resolving_turn_id(),
            })
    {
        return Err(DiscussionSettlementError::IdentityMismatch);
    }
    if let Some(parent) = job.state().parent() {
        let request = DiscussionParentExecutionRequest {
            child_gate: gate,
            parent_thread_id: job.parent_thread_id(),
            input_id: parent.accepted_input_id(),
            turn_id: parent.turn_id(),
            context_owner: job.context_owner_id(),
            context_digest: job.context_digest(),
            resolution: job.resolution().as_str().to_owned(),
        };
        let observed = match access {
            Access::Candidate(candidate) => {
                syndic.prepare_discussion_parent_execution_candidate(candidate, request)?
            }
            Access::Ordinary(store) => {
                syndic.prepare_discussion_parent_execution(store, request)?
            }
        };
        if let Some(expected) = job.state().parent_cas() {
            let DiscussionParentExecution::Proven(proof) = observed else {
                return Err(DiscussionSettlementError::IdentityMismatch);
            };
            let source = match proof.disposition() {
                DiscussionParentExecutionDisposition::Accepted(source) => Some(source),
                DiscussionParentExecutionDisposition::Terminal { cas, .. } => cas.as_ref(),
            };
            if !source.is_some_and(|source| {
                source.thread_id() == expected.thread_id() && source.turn_id() == expected.turn_id()
            }) {
                return Err(DiscussionSettlementError::IdentityMismatch);
            }
        }
    } else {
        access.parent(syndic, job, gate)?;
    }
    if access.revision()? != before {
        return Err(DiscussionSettlementError::ConcurrentChange);
    }
    Ok(())
}

fn check_cancelled(
    cancellation: &CommandCancellation,
) -> Result<(), HandoffCandidateConvergenceError> {
    if cancellation.is_cancelled() {
        Err(HandoffCandidateConvergenceError::Cancelled)
    } else {
        Ok(())
    }
}
fn increment(value: &mut u64, amount: u64) -> Result<(), HandoffCandidateConvergenceError> {
    *value = value
        .checked_add(amount)
        .ok_or(HandoffCandidateConvergenceError::CounterExhausted)?;
    Ok(())
}
