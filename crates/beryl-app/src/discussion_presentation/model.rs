use super::*;
use crate::discussion_settlement::{DiscussionSettlementError, presentation::LocalBoundary};
use beryl_home_store::HomeMutationObservation;
use beryl_model::{HomeRevision, JobId, JobRevision, ResolutionIntentId};
use beryl_state::ResolutionAttemptOrdinal;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionStatus {
    Open,
    ResolutionPending,
    HandingOff,
    HandoffFailed,
    Unavailable,
    Archived,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionComposerContribution {
    Writable,
    Inert,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscussionAttemptIdentity {
    pub(super) job: JobId,
    pub(super) intent: ResolutionIntentId,
    pub(super) ordinal: ResolutionAttemptOrdinal,
}
impl DiscussionAttemptIdentity {
    pub fn job_id(self) -> JobId {
        self.job
    }
    pub fn intent_id(self) -> ResolutionIntentId {
        self.intent
    }
    pub fn attempt_ordinal(self) -> ResolutionAttemptOrdinal {
        self.ordinal
    }
}

#[derive(Clone)]
pub(super) struct Fence {
    pub source: Weak<dyn DiscussionPresentationAccess>,
    pub mutations: HomeMutationObservation,
    pub local: Weak<LocalBoundary>,
    pub local_revision: u64,
}
impl Fence {
    pub fn elect_local<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, DiscussionPresentationError> {
        if self.source.upgrade().is_none_or(|source| !source.current()) {
            return Err(DiscussionPresentationError::Retired);
        }
        self.local
            .upgrade()
            .ok_or(DiscussionPresentationError::Retired)?
            .elect(self.local_revision, publish)
            .map_err(Into::into)
    }
    pub fn elect<T>(&self, publish: impl FnOnce() -> T) -> Result<T, DiscussionPresentationError> {
        if self.source.upgrade().is_none_or(|source| !source.current()) {
            return Err(DiscussionPresentationError::Retired);
        }
        let local = self
            .local
            .upgrade()
            .ok_or(DiscussionPresentationError::Retired)?;
        self.mutations
            .try_elect(|| local.elect(self.local_revision, publish))
            .map_err(|_| DiscussionPresentationError::Changed)?
            .map_err(Into::into)
    }
}

#[derive(Clone)]
pub struct DiscussionRetryDescriptor {
    pub(super) home: BerylHomeId,
    pub(super) generation: HomeGeneration,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) thread: SyndicThreadId,
    pub(super) attempt: DiscussionAttemptIdentity,
    pub(super) job_revision: JobRevision,
    pub(super) home_revision: HomeRevision,
    pub(super) fence: Fence,
}
impl DiscussionRetryDescriptor {
    pub fn thread_id(&self) -> SyndicThreadId {
        self.thread
    }
    pub fn job_id(&self) -> JobId {
        self.attempt.job
    }
    pub fn job_revision(&self) -> JobRevision {
        self.job_revision
    }
}
impl PartialEq for DiscussionRetryDescriptor {
    fn eq(&self, other: &Self) -> bool {
        (
            self.home,
            self.generation,
            self.service_generation,
            self.thread,
            self.attempt,
            self.job_revision,
            self.home_revision,
            self.fence.local_revision,
        ) == (
            other.home,
            other.generation,
            other.service_generation,
            other.thread,
            other.attempt,
            other.job_revision,
            other.home_revision,
            other.fence.local_revision,
        )
    }
}
impl Eq for DiscussionRetryDescriptor {}
impl std::fmt::Debug for DiscussionRetryDescriptor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DiscussionRetryDescriptor")
            .field("thread", &self.thread)
            .field("attempt", &self.attempt)
            .field("revision", &self.job_revision)
            .finish()
    }
}

#[derive(Clone)]
pub struct DiscussionPresentationSnapshot {
    pub(super) home: BerylHomeId,
    pub(super) generation: HomeGeneration,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) thread: SyndicThreadId,
    pub(super) status: Option<DiscussionStatus>,
    pub(super) composer: DiscussionComposerContribution,
    pub(super) attempt: Option<DiscussionAttemptIdentity>,
    pub(super) retry: Option<DiscussionRetryDescriptor>,
    pub(super) retry_pending: bool,
    pub(super) detail: Option<String>,
    pub(super) fence: Fence,
}
impl DiscussionPresentationSnapshot {
    pub fn home_observation(&self) -> HomeMutationObservation {
        self.fence.mutations.clone()
    }

    pub fn same_fence(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.fence.source, &other.fence.source)
            && Weak::ptr_eq(&self.fence.local, &other.fence.local)
            && self.fence.local_revision == other.fence.local_revision
            && self.fence.mutations.same_interval(&other.fence.mutations)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_with_presentation(
        &self,
        status: DiscussionStatus,
        composer: DiscussionComposerContribution,
        detail: Option<&str>,
    ) -> Self {
        let mut snapshot = self.clone();
        snapshot.status = Some(status);
        snapshot.composer = composer;
        snapshot.detail = detail.map(|detail| {
            let mut end = detail.len().min(2048);
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail[..end].to_owned()
        });
        snapshot
    }
    pub fn home_id(&self) -> BerylHomeId {
        self.home
    }
    pub fn home_generation(&self) -> HomeGeneration {
        self.generation
    }
    pub fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }
    pub fn thread_id(&self) -> SyndicThreadId {
        self.thread
    }
    pub fn status(&self) -> Option<DiscussionStatus> {
        self.status
    }
    pub fn composer(&self) -> DiscussionComposerContribution {
        self.composer
    }
    pub fn attempt(&self) -> Option<DiscussionAttemptIdentity> {
        self.attempt
    }
    pub fn retry(&self) -> Option<&DiscussionRetryDescriptor> {
        self.retry.as_ref()
    }
    pub fn retry_pending(&self) -> bool {
        self.retry_pending
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub fn try_elect<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, DiscussionPresentationError> {
        self.fence.elect(publish)
    }
    pub fn try_elect_local<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, DiscussionPresentationError> {
        self.fence.elect_local(publish)
    }
    pub fn current(&self) -> bool {
        self.try_elect(|| ()).is_ok()
    }
}
impl PartialEq for DiscussionPresentationSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.home == other.home
            && self.generation == other.generation
            && self.service_generation == other.service_generation
            && self.thread == other.thread
            && self.status == other.status
            && self.composer == other.composer
            && self.attempt == other.attempt
            && self.retry == other.retry
            && self.retry_pending == other.retry_pending
            && self.detail == other.detail
    }
}
impl Eq for DiscussionPresentationSnapshot {}
impl std::fmt::Debug for DiscussionPresentationSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DiscussionPresentationSnapshot")
            .field("thread", &self.thread)
            .field("status", &self.status)
            .field("composer", &self.composer)
            .field("attempt", &self.attempt)
            .field("retry", &self.retry)
            .field("retry_pending", &self.retry_pending)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscussionPresentationError {
    #[error("discussion presentation source retired")]
    Retired,
    #[error("discussion presentation changed while being observed")]
    Changed,
    #[error("discussion presentation source identities disagree")]
    SourceMismatch,
    #[error(transparent)]
    Read(#[from] beryl_home_store::ReadError),
    #[error(transparent)]
    Syndic(#[from] syndic_storage::SyndicReadError),
    #[error(transparent)]
    Settlement(#[from] DiscussionSettlementError),
}
