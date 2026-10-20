mod model;
mod read;
mod retry;
pub use model::*;
pub use retry::{DiscussionRetryObservation, DiscussionRetryOutcome, PreparedDiscussionRetry};

use crate::{
    cas_projection::ProjectionServiceGeneration, discussion_settlement::DiscussionSettlementService,
};
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeGeneration, HomeMutationObserver};
use beryl_model::{BerylHomeId, SyndicThreadId};
use std::sync::{Arc, Weak};

pub(crate) trait DiscussionPresentationAccess: Send + Sync {
    fn service(&self) -> Option<DiscussionSettlementService>;
    fn current(&self) -> bool;
}

#[derive(Clone)]
pub struct DiscussionPresentationReader {
    source: Weak<dyn DiscussionPresentationAccess>,
    mutations: HomeMutationObserver,
    home: BerylHomeId,
    generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
}

impl DiscussionPresentationReader {
    pub(crate) fn new(
        source: Arc<dyn DiscussionPresentationAccess>,
        mutations: HomeMutationObserver,
        home: BerylHomeId,
        generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
    ) -> Self {
        Self {
            source: Arc::downgrade(&source),
            mutations,
            home,
            generation,
            service_generation,
        }
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
    pub fn current(&self) -> bool {
        self.source.upgrade().is_some_and(|source| source.current())
    }

    pub fn observe(
        &self,
        thread: SyndicThreadId,
    ) -> Result<DiscussionPresentationSnapshot, DiscussionPresentationError> {
        let source = self
            .source
            .upgrade()
            .ok_or(DiscussionPresentationError::Retired)?;
        if !source.current() {
            return Err(DiscussionPresentationError::Retired);
        }
        let service = source
            .service()
            .ok_or(DiscussionPresentationError::Retired)?;
        read::observe(
            self,
            &service,
            read::Access::Ordinary(service.home()),
            thread,
        )
    }

    pub fn observe_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        thread: SyndicThreadId,
    ) -> Result<DiscussionPresentationSnapshot, DiscussionPresentationError> {
        if access.home_id() != self.home || access.generation() != self.generation {
            return Err(DiscussionPresentationError::Retired);
        }
        let source = self
            .source
            .upgrade()
            .ok_or(DiscussionPresentationError::Retired)?;
        let service = source
            .service()
            .ok_or(DiscussionPresentationError::Retired)?;
        read::observe(self, &service, read::Access::Candidate(access), thread)
    }
}
