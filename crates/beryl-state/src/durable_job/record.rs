use beryl_model::{JobId, JobRevision, ResolutionIntentId, SyndicThreadId, SyndicTurnId};

use super::{
    DiscussionContextDigest, DiscussionContextOwnerId, HandoffFailureEvidence, HandoffFailureKind,
    ParentCasIdentity, ParentHandoffIdentity, ParentQueueOrdinal, ResolutionAttemptOrdinal,
    ResolutionRequestIdentity, ResolutionText,
};

/// Derives the stable handoff job identity owned by one admitted intent.
#[must_use]
pub const fn branch_handoff_job_id(intent_id: ResolutionIntentId) -> JobId {
    JobId::from_bytes(*intent_id.as_bytes())
}

/// Immutable caller-admitted facts for one fresh branch-handoff attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchHandoffJobAdmission {
    pub(super) intent_id: ResolutionIntentId,
    pub(super) attempt_ordinal: ResolutionAttemptOrdinal,
    pub(super) discussion_thread_id: SyndicThreadId,
    pub(super) parent_thread_id: SyndicThreadId,
    pub(super) context_owner_id: DiscussionContextOwnerId,
    pub(super) context_digest: DiscussionContextDigest,
    pub(super) resolving_turn_id: SyndicTurnId,
    pub(super) request: ResolutionRequestIdentity,
    pub(super) parent_queue_ordinal: ParentQueueOrdinal,
    pub(super) resolution: ResolutionText,
}

impl BranchHandoffJobAdmission {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        intent_id: ResolutionIntentId,
        attempt_ordinal: ResolutionAttemptOrdinal,
        discussion_thread_id: SyndicThreadId,
        parent_thread_id: SyndicThreadId,
        context_owner_id: DiscussionContextOwnerId,
        context_digest: DiscussionContextDigest,
        resolving_turn_id: SyndicTurnId,
        request: ResolutionRequestIdentity,
        parent_queue_ordinal: ParentQueueOrdinal,
        resolution: ResolutionText,
    ) -> Self {
        Self {
            intent_id,
            attempt_ordinal,
            discussion_thread_id,
            parent_thread_id,
            context_owner_id,
            context_digest,
            resolving_turn_id,
            request,
            parent_queue_ordinal,
            resolution,
        }
    }

    #[must_use]
    pub const fn job_id(&self) -> JobId {
        branch_handoff_job_id(self.intent_id)
    }

    #[must_use]
    pub const fn intent_id(&self) -> ResolutionIntentId {
        self.intent_id
    }

    #[must_use]
    pub const fn attempt_ordinal(&self) -> ResolutionAttemptOrdinal {
        self.attempt_ordinal
    }

    #[must_use]
    pub const fn discussion_thread_id(&self) -> SyndicThreadId {
        self.discussion_thread_id
    }

    #[must_use]
    pub const fn parent_thread_id(&self) -> SyndicThreadId {
        self.parent_thread_id
    }

    #[must_use]
    pub const fn context_owner_id(&self) -> DiscussionContextOwnerId {
        self.context_owner_id
    }

    #[must_use]
    pub const fn context_digest(&self) -> DiscussionContextDigest {
        self.context_digest
    }

    #[must_use]
    pub const fn resolving_turn_id(&self) -> SyndicTurnId {
        self.resolving_turn_id
    }

    #[must_use]
    pub const fn request(&self) -> &ResolutionRequestIdentity {
        &self.request
    }

    #[must_use]
    pub const fn parent_queue_ordinal(&self) -> ParentQueueOrdinal {
        self.parent_queue_ordinal
    }

    #[must_use]
    pub const fn resolution(&self) -> &ResolutionText {
        &self.resolution
    }
}

/// Immutable admission result returned for a repeated correlated tool request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolutionRequestAdmission {
    pub(super) job_id: JobId,
    pub(super) intent_id: ResolutionIntentId,
    pub(super) discussion_thread_id: SyndicThreadId,
    pub(super) attempt_ordinal: ResolutionAttemptOrdinal,
}

impl ResolutionRequestAdmission {
    pub(super) const fn from_job(job: &BranchHandoffJobRecord) -> Self {
        Self {
            job_id: job.job_id,
            intent_id: job.intent_id,
            discussion_thread_id: job.discussion_thread_id,
            attempt_ordinal: job.attempt_ordinal,
        }
    }

    #[must_use]
    pub const fn job_id(self) -> JobId {
        self.job_id
    }

    #[must_use]
    pub const fn intent_id(self) -> ResolutionIntentId {
        self.intent_id
    }

    #[must_use]
    pub const fn discussion_thread_id(self) -> SyndicThreadId {
        self.discussion_thread_id
    }

    #[must_use]
    pub const fn attempt_ordinal(self) -> ResolutionAttemptOrdinal {
        self.attempt_ordinal
    }
}

/// Exact latest-attempt pointer for one discussion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatestBranchHandoffAttempt {
    pub(super) job_id: JobId,
    pub(super) attempt_ordinal: ResolutionAttemptOrdinal,
}

impl LatestBranchHandoffAttempt {
    pub(super) const fn from_job(job: &BranchHandoffJobRecord) -> Self {
        Self {
            job_id: job.job_id,
            attempt_ordinal: job.attempt_ordinal,
        }
    }

    #[must_use]
    pub const fn job_id(self) -> JobId {
        self.job_id
    }

    #[must_use]
    pub const fn attempt_ordinal(self) -> ResolutionAttemptOrdinal {
        self.attempt_ordinal
    }
}

/// Exact lifecycle name persisted for one branch-handoff job.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BranchHandoffJobLifecycle {
    WaitingResolvingTurn,
    WaitingParent,
    StartingParent,
    ParentActive,
    RetryableFailed,
    TerminalFailed,
    Succeeded,
}

impl BranchHandoffJobLifecycle {
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            Self::WaitingResolvingTurn
                | Self::WaitingParent
                | Self::StartingParent
                | Self::ParentActive
                | Self::RetryableFailed
        )
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::TerminalFailed | Self::Succeeded)
    }
}

/// Last exact non-failure checkpoint retained across retry or terminal failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BranchHandoffCheckpoint {
    WaitingResolvingTurn,
    WaitingParent,
    StartingParent {
        parent: ParentHandoffIdentity,
    },
    ParentActive {
        parent: ParentHandoffIdentity,
        cas: ParentCasIdentity,
    },
}

impl BranchHandoffCheckpoint {
    #[must_use]
    pub const fn lifecycle(&self) -> BranchHandoffJobLifecycle {
        match self {
            Self::WaitingResolvingTurn => BranchHandoffJobLifecycle::WaitingResolvingTurn,
            Self::WaitingParent => BranchHandoffJobLifecycle::WaitingParent,
            Self::StartingParent { .. } => BranchHandoffJobLifecycle::StartingParent,
            Self::ParentActive { .. } => BranchHandoffJobLifecycle::ParentActive,
        }
    }

    pub(super) fn into_state(self) -> BranchHandoffJobState {
        match self {
            Self::WaitingResolvingTurn => BranchHandoffJobState::WaitingResolvingTurn,
            Self::WaitingParent => BranchHandoffJobState::WaitingParent,
            Self::StartingParent { parent } => BranchHandoffJobState::StartingParent { parent },
            Self::ParentActive { parent, cas } => {
                BranchHandoffJobState::ParentActive { parent, cas }
            }
        }
    }

    #[must_use]
    pub const fn parent(&self) -> Option<ParentHandoffIdentity> {
        match self {
            Self::StartingParent { parent } | Self::ParentActive { parent, .. } => Some(*parent),
            Self::WaitingResolvingTurn | Self::WaitingParent => None,
        }
    }

    #[must_use]
    pub const fn parent_cas(&self) -> Option<&ParentCasIdentity> {
        match self {
            Self::ParentActive { cas, .. } => Some(cas),
            _ => None,
        }
    }
}

pub(super) const fn failure_state_is_compatible(
    failure_lifecycle: BranchHandoffJobLifecycle,
    checkpoint: &BranchHandoffCheckpoint,
    kind: HandoffFailureKind,
) -> bool {
    let disposition_matches = match failure_lifecycle {
        BranchHandoffJobLifecycle::RetryableFailed => kind.is_retryable(),
        BranchHandoffJobLifecycle::TerminalFailed => kind.is_terminal(),
        _ => false,
    };
    if !disposition_matches {
        return false;
    }

    let checkpoint_lifecycle = checkpoint.lifecycle();
    match kind {
        HandoffFailureKind::CasRejectedBeforeAcceptance => {
            matches!(
                checkpoint_lifecycle,
                BranchHandoffJobLifecycle::StartingParent
            )
        }
        HandoffFailureKind::UnrecoverablePostAppend => matches!(
            checkpoint_lifecycle,
            BranchHandoffJobLifecycle::StartingParent | BranchHandoffJobLifecycle::ParentActive
        ),
        HandoffFailureKind::ParentInterrupted
        | HandoffFailureKind::ParentIncomplete
        | HandoffFailureKind::ParentTerminalFailure => {
            matches!(
                checkpoint_lifecycle,
                BranchHandoffJobLifecycle::ParentActive
            )
        }
        _ => true,
    }
}

/// Detailed state retaining the exact identities needed to resume without duplication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BranchHandoffJobState {
    WaitingResolvingTurn,
    WaitingParent,
    StartingParent {
        parent: ParentHandoffIdentity,
    },
    ParentActive {
        parent: ParentHandoffIdentity,
        cas: ParentCasIdentity,
    },
    RetryableFailed {
        resume: BranchHandoffCheckpoint,
        evidence: HandoffFailureEvidence,
    },
    TerminalFailed {
        stopped_at: BranchHandoffCheckpoint,
        evidence: HandoffFailureEvidence,
    },
    Succeeded {
        parent: ParentHandoffIdentity,
        cas: ParentCasIdentity,
    },
}

impl BranchHandoffJobState {
    #[must_use]
    pub const fn lifecycle(&self) -> BranchHandoffJobLifecycle {
        match self {
            Self::WaitingResolvingTurn => BranchHandoffJobLifecycle::WaitingResolvingTurn,
            Self::WaitingParent => BranchHandoffJobLifecycle::WaitingParent,
            Self::StartingParent { .. } => BranchHandoffJobLifecycle::StartingParent,
            Self::ParentActive { .. } => BranchHandoffJobLifecycle::ParentActive,
            Self::RetryableFailed { .. } => BranchHandoffJobLifecycle::RetryableFailed,
            Self::TerminalFailed { .. } => BranchHandoffJobLifecycle::TerminalFailed,
            Self::Succeeded { .. } => BranchHandoffJobLifecycle::Succeeded,
        }
    }

    pub(super) fn checkpoint(&self) -> Option<BranchHandoffCheckpoint> {
        match self {
            Self::WaitingResolvingTurn => Some(BranchHandoffCheckpoint::WaitingResolvingTurn),
            Self::WaitingParent => Some(BranchHandoffCheckpoint::WaitingParent),
            Self::StartingParent { parent } => {
                Some(BranchHandoffCheckpoint::StartingParent { parent: *parent })
            }
            Self::ParentActive { parent, cas } => Some(BranchHandoffCheckpoint::ParentActive {
                parent: *parent,
                cas: cas.clone(),
            }),
            Self::RetryableFailed { resume, .. } => Some(resume.clone()),
            Self::TerminalFailed { .. } | Self::Succeeded { .. } => None,
        }
    }

    #[must_use]
    pub const fn parent(&self) -> Option<ParentHandoffIdentity> {
        match self {
            Self::StartingParent { parent }
            | Self::ParentActive { parent, .. }
            | Self::Succeeded { parent, .. } => Some(*parent),
            Self::RetryableFailed { resume, .. } => resume.parent(),
            Self::TerminalFailed { stopped_at, .. } => stopped_at.parent(),
            Self::WaitingResolvingTurn | Self::WaitingParent => None,
        }
    }

    #[must_use]
    pub const fn parent_cas(&self) -> Option<&ParentCasIdentity> {
        match self {
            Self::ParentActive { cas, .. } | Self::Succeeded { cas, .. } => Some(cas),
            Self::RetryableFailed { resume, .. } => resume.parent_cas(),
            Self::TerminalFailed { stopped_at, .. } => stopped_at.parent_cas(),
            _ => None,
        }
    }

    #[must_use]
    pub const fn failure_evidence(&self) -> Option<&HandoffFailureEvidence> {
        match self {
            Self::RetryableFailed { evidence, .. } | Self::TerminalFailed { evidence, .. } => {
                Some(evidence)
            }
            _ => None,
        }
    }
}

/// Durable authoritative record for one typed branch-handoff attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchHandoffJobRecord {
    pub(super) job_id: JobId,
    pub(super) intent_id: ResolutionIntentId,
    pub(super) attempt_ordinal: ResolutionAttemptOrdinal,
    pub(super) discussion_thread_id: SyndicThreadId,
    pub(super) parent_thread_id: SyndicThreadId,
    pub(super) context_owner_id: DiscussionContextOwnerId,
    pub(super) context_digest: DiscussionContextDigest,
    pub(super) resolving_turn_id: SyndicTurnId,
    pub(super) request: ResolutionRequestIdentity,
    pub(super) parent_queue_ordinal: ParentQueueOrdinal,
    pub(super) resolution: ResolutionText,
    pub(super) state: BranchHandoffJobState,
    pub(super) revision: JobRevision,
}

impl BranchHandoffJobRecord {
    pub(super) fn initial(admission: &BranchHandoffJobAdmission) -> Self {
        Self {
            job_id: admission.job_id(),
            intent_id: admission.intent_id,
            attempt_ordinal: admission.attempt_ordinal,
            discussion_thread_id: admission.discussion_thread_id,
            parent_thread_id: admission.parent_thread_id,
            context_owner_id: admission.context_owner_id,
            context_digest: admission.context_digest,
            resolving_turn_id: admission.resolving_turn_id,
            request: admission.request.clone(),
            parent_queue_ordinal: admission.parent_queue_ordinal,
            resolution: admission.resolution.clone(),
            state: BranchHandoffJobState::WaitingResolvingTurn,
            revision: JobRevision::new(1).expect("initial job revision is nonzero"),
        }
    }

    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    #[must_use]
    pub const fn intent_id(&self) -> ResolutionIntentId {
        self.intent_id
    }

    #[must_use]
    pub const fn attempt_ordinal(&self) -> ResolutionAttemptOrdinal {
        self.attempt_ordinal
    }

    #[must_use]
    pub const fn discussion_thread_id(&self) -> SyndicThreadId {
        self.discussion_thread_id
    }

    #[must_use]
    pub const fn parent_thread_id(&self) -> SyndicThreadId {
        self.parent_thread_id
    }

    #[must_use]
    pub const fn context_owner_id(&self) -> DiscussionContextOwnerId {
        self.context_owner_id
    }

    #[must_use]
    pub const fn context_digest(&self) -> DiscussionContextDigest {
        self.context_digest
    }

    #[must_use]
    pub const fn resolving_turn_id(&self) -> SyndicTurnId {
        self.resolving_turn_id
    }

    #[must_use]
    pub const fn request(&self) -> &ResolutionRequestIdentity {
        &self.request
    }

    #[must_use]
    pub const fn parent_queue_ordinal(&self) -> ParentQueueOrdinal {
        self.parent_queue_ordinal
    }

    #[must_use]
    pub const fn resolution(&self) -> &ResolutionText {
        &self.resolution
    }

    #[must_use]
    pub const fn state(&self) -> &BranchHandoffJobState {
        &self.state
    }

    #[must_use]
    pub const fn lifecycle(&self) -> BranchHandoffJobLifecycle {
        self.state.lifecycle()
    }

    #[must_use]
    pub const fn revision(&self) -> JobRevision {
        self.revision
    }
}
