use super::{model::Fence, *};
use crate::discussion_settlement::{
    DiscussionSettlementService,
    presentation::{LocalDisposition, LocalFact},
};
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeStore};
use beryl_state::BranchHandoffJobLifecycle as JobState;
use syndic_storage::{DiscussionHandoffGateState as GateState, *};

#[derive(Clone, Copy)]
pub(super) enum Access<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}
impl Access<'_> {
    fn revision(self) -> Result<beryl_model::HomeRevision, DiscussionPresentationError> {
        Ok(match self {
            Self::Ordinary(home) => home.home_revision()?,
            Self::Candidate(access) => access.home_revision()?,
        })
    }
}

pub(super) fn observe(
    reader: &DiscussionPresentationReader,
    service: &DiscussionSettlementService,
    access: Access<'_>,
    thread: beryl_model::SyndicThreadId,
) -> Result<DiscussionPresentationSnapshot, DiscussionPresentationError> {
    let (state, storage, operations) = service.presentation_parts();
    if let Some(audit) = operations.selected_presentation_audit(thread)? {
        match access {
            Access::Ordinary(home) => {
                audit.reconcile(home, storage, state)?;
            }
            Access::Candidate(candidate) => {
                audit.reconcile_candidate(candidate, storage, state)?;
            }
        }
    }
    let boundary = operations.presentation_boundary();
    let local = boundary
        .upgrade()
        .ok_or(DiscussionPresentationError::Retired)?;
    let local_revision = local.revision()?;
    let mutations = reader
        .mutations
        .observe()
        .map_err(|_| DiscussionPresentationError::Changed)?;
    let revision = access.revision()?;
    let limit = SyndicPointReadLimit::new(1_000_000).expect("fixed point limit");
    macro_rules! point {
        ($ordinary:ident, $candidate:ident, $id:expr) => {
            match access {
                Access::Ordinary(home) => storage.$ordinary(home, $id, limit)?,
                Access::Candidate(candidate) => storage.$candidate(candidate, $id, limit)?,
            }
        };
    }
    let record = point!(thread, thread_candidate, thread)
        .ok_or(DiscussionPresentationError::SourceMismatch)?;
    let attributes = point!(thread_attributes, thread_attributes_candidate, thread)
        .ok_or(DiscussionPresentationError::SourceMismatch)?;
    let input_gate = point!(input_gate, input_gate_candidate, thread)
        .ok_or(DiscussionPresentationError::SourceMismatch)?;
    let gate = point!(
        discussion_handoff_gate,
        discussion_handoff_gate_candidate,
        thread
    );
    if record.id() != thread || attributes.thread_id() != thread || input_gate.thread_id() != thread
    {
        return Err(DiscussionPresentationError::SourceMismatch);
    }
    let latest = match access {
        Access::Ordinary(home) => state.durable_jobs().latest_attempt(home, thread)?,
        Access::Candidate(candidate) => state
            .durable_jobs()
            .latest_attempt_candidate(candidate, thread)?,
    };
    let fact = operations.presentation_fact(thread)?;
    let preparing =
        operations.presentation_preparing(thread, latest.map(|latest| latest.job_id()))?;
    let mut snapshot = DiscussionPresentationSnapshot {
        home: reader.home,
        generation: reader.generation,
        service_generation: reader.service_generation,
        thread,
        status: None,
        composer: DiscussionComposerContribution::Writable,
        attempt: None,
        retry: None,
        retry_pending: false,
        detail: None,
        fence: Fence {
            source: reader.source.clone(),
            mutations,
            local: boundary,
            local_revision,
        },
    };
    if let Some(owner) = record.context_owner_id() {
        let parent = record
            .parent_thread_id()
            .ok_or(DiscussionPresentationError::SourceMismatch)?;
        let gate = gate.ok_or(DiscussionPresentationError::SourceMismatch)?;
        if gate.thread_id() != thread || attributes.archive() == ThreadArchiveState::Ordinary {
            return Err(DiscussionPresentationError::SourceMismatch);
        }
        let context_digest = point!(context_envelope, context_envelope_candidate, owner)
            .ok_or(DiscussionPresentationError::SourceMismatch)?
            .envelope()
            .descriptor()
            .digest();
        if let Some(fact) = fact {
            if fact.home != reader.home
                || fact.child != thread
                || fact.parent != parent
                || fact.owner != owner
            {
                return Err(DiscussionPresentationError::SourceMismatch);
            }
            snapshot.attempt = Some(attempt(fact));
        }
        if fact.is_some_and(|fact| fact.disposition == LocalDisposition::Collision) {
            snapshot.status = Some(DiscussionStatus::Unavailable);
            snapshot.composer = DiscussionComposerContribution::Inert;
            snapshot.detail = Some("The exact handoff outcome is unavailable. Its evidence is retained. Use established same-home recovery or bounded diagnostic reporting.".into());
        } else {
            let job = match latest {
                Some(latest) => Some(
                    match access {
                        Access::Ordinary(home) => {
                            state.durable_jobs().job(home, latest.job_id())?
                        }
                        Access::Candidate(candidate) => state
                            .durable_jobs()
                            .job_candidate(candidate, latest.job_id())?,
                    }
                    .ok_or(DiscussionPresentationError::SourceMismatch)?,
                ),
                None => None,
            };
            if let Some(job) = job.as_ref() {
                if job.discussion_thread_id() != thread
                    || job.parent_thread_id() != parent
                    || job.context_owner_id() != owner
                    || job.context_digest() != context_digest
                    || latest.is_none_or(|latest| latest.attempt_ordinal() != job.attempt_ordinal())
                    || fact.is_some_and(|fact| {
                        fact.job != job.job_id()
                            || fact.intent != job.intent_id()
                            || fact.attempt != job.attempt_ordinal()
                    })
                {
                    return Err(DiscussionPresentationError::SourceMismatch);
                }
                snapshot.attempt = Some(DiscussionAttemptIdentity {
                    job: job.job_id(),
                    intent: job.intent_id(),
                    ordinal: job.attempt_ordinal(),
                });
                if job.lifecycle().is_live() {
                    if gate.state()
                        != (GateState::Pending {
                            intent_id: job.intent_id(),
                            job_id: job.job_id(),
                            resolving_turn_id: job.resolving_turn_id(),
                        })
                        || attributes.archive() != ThreadArchiveState::BranchDiscussionOpen
                    {
                        return Err(DiscussionPresentationError::SourceMismatch);
                    }
                } else if gate.state() != GateState::Open {
                    return Err(DiscussionPresentationError::SourceMismatch);
                }
                snapshot.status = Some(match job.lifecycle() {
                    JobState::WaitingResolvingTurn | JobState::WaitingParent => {
                        DiscussionStatus::ResolutionPending
                    }
                    JobState::StartingParent | JobState::ParentActive => {
                        let parent = job
                            .state()
                            .parent()
                            .ok_or(DiscussionPresentationError::SourceMismatch)?;
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
                            Access::Ordinary(home) => {
                                storage.prepare_discussion_parent_execution(home, request)?
                            }
                            Access::Candidate(candidate) => storage
                                .prepare_discussion_parent_execution_candidate(
                                    candidate, request,
                                )?,
                        };
                        match observed {
                            DiscussionParentExecution::Proven(proof) => match proof.disposition() {
                                DiscussionParentExecutionDisposition::Accepted(cas) => {
                                    if job.state().parent_cas().is_some_and(|expected| {
                                        expected.thread_id() != cas.thread_id()
                                            || expected.turn_id() != cas.turn_id()
                                    }) {
                                        return Err(DiscussionPresentationError::SourceMismatch);
                                    }
                                    DiscussionStatus::HandingOff
                                }
                                DiscussionParentExecutionDisposition::Terminal { .. } => {
                                    DiscussionStatus::ResolutionPending
                                }
                            },
                            DiscussionParentExecution::Waiting
                                if job.lifecycle() == JobState::StartingParent =>
                            {
                                DiscussionStatus::ResolutionPending
                            }
                            DiscussionParentExecution::Waiting => {
                                return Err(DiscussionPresentationError::SourceMismatch);
                            }
                        }
                    }
                    JobState::RetryableFailed | JobState::TerminalFailed => {
                        DiscussionStatus::HandoffFailed
                    }
                    JobState::Succeeded => {
                        match attributes.archive() {
                            ThreadArchiveState::BranchDiscussionArchived {
                                handoff_job_id, ..
                            } if handoff_job_id == job.job_id() => DiscussionStatus::Archived,
                            _ => return Err(DiscussionPresentationError::SourceMismatch),
                        }
                    }
                });
                if job.lifecycle() != JobState::Succeeded
                    && attributes.archive() != ThreadArchiveState::BranchDiscussionOpen
                {
                    return Err(DiscussionPresentationError::SourceMismatch);
                }
                snapshot.detail = job.state().failure_evidence().map(|failure| failure.detail().unwrap_or("The handoff did not complete. The exact accepted attempt and any existing parent input are retained.").to_owned());
                if job.lifecycle() == JobState::RetryableFailed {
                    snapshot.retry = Some(DiscussionRetryDescriptor {
                        home: reader.home,
                        generation: reader.generation,
                        service_generation: reader.service_generation,
                        thread,
                        attempt: snapshot.attempt.unwrap(),
                        job_revision: job.revision(),
                        home_revision: revision,
                        fence: snapshot.fence.clone(),
                    });
                }
            } else {
                if gate.state() != GateState::Open
                    || attributes.archive() != ThreadArchiveState::BranchDiscussionOpen
                    || fact.is_some_and(|fact| !fact.admission)
                {
                    return Err(DiscussionPresentationError::SourceMismatch);
                }
                snapshot.status = Some(DiscussionStatus::Open);
            }
            let pending = preparing
                || fact.is_some_and(|fact| {
                    matches!(
                        fact.disposition,
                        LocalDisposition::Prepared | LocalDisposition::Pending
                    )
                });
            snapshot.retry_pending = pending && snapshot.retry.is_some();
            snapshot.composer = match snapshot.status {
                Some(DiscussionStatus::Archived) => DiscussionComposerContribution::ReadOnly,
                Some(DiscussionStatus::Open) if !pending => {
                    DiscussionComposerContribution::Writable
                }
                Some(DiscussionStatus::HandoffFailed)
                    if job
                        .as_ref()
                        .is_some_and(|job| job.lifecycle() == JobState::TerminalFailed)
                        && !pending =>
                {
                    DiscussionComposerContribution::Writable
                }
                _ => DiscussionComposerContribution::Inert,
            };
        }
    } else if gate.is_some()
        || latest.is_some()
        || fact.is_some()
        || attributes.archive() != ThreadArchiveState::Ordinary
    {
        return Err(DiscussionPresentationError::SourceMismatch);
    }
    if access.revision()? != revision || local.revision()? != local_revision {
        return Err(DiscussionPresentationError::Changed);
    }
    snapshot.fence.elect(|| ())?;
    Ok(snapshot)
}

fn attempt(fact: LocalFact) -> DiscussionAttemptIdentity {
    DiscussionAttemptIdentity {
        job: fact.job,
        intent: fact.intent,
        ordinal: fact.attempt,
    }
}
