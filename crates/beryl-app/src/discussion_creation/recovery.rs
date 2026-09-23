use beryl_home_store::{HomeCandidateRecoveryAccess, ReconciliationResolution};
use beryl_state::CatalogInitialStatus;
use syndic_storage::ThreadCreationStatus;

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionCreationAuditOutcome {
    Pending,
    NotCommitted,
    Created(CreatedDiscussion),
    Collision,
}

impl DiscussionCreationAudit {
    pub fn reconcile(
        &self,
        store: &HomeStore,
        syndic: &SyndicStorage,
        state: &BerylState,
    ) -> Result<DiscussionCreationAuditOutcome, DiscussionCreationError> {
        self.reconcile_with_access(Access::Ordinary(store), syndic, state)
    }

    pub fn reconcile_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        syndic: &SyndicStorage,
        state: &BerylState,
    ) -> Result<DiscussionCreationAuditOutcome, DiscussionCreationError> {
        self.reconcile_with_access(Access::Candidate(access), syndic, state)
    }

    fn reconcile_with_access(
        &self,
        access: Access<'_>,
        syndic: &SyndicStorage,
        state: &BerylState,
    ) -> Result<DiscussionCreationAuditOutcome, DiscussionCreationError> {
        let mut disposition = self
            .0
            .disposition
            .lock()
            .map_err(|_| DiscussionCreationError::CustodyUnavailable)?;
        if access.home_id() != self.0.home_id {
            return Err(DiscussionCreationError::ForeignHome);
        }
        let natural = access.natural(self, syndic, state)?;
        if matches!(*disposition, Disposition::Prepared) {
            return Ok(DiscussionCreationAuditOutcome::Pending);
        }
        if matches!(*disposition, Disposition::Collision) {
            return Ok(DiscussionCreationAuditOutcome::Collision);
        }
        if let Disposition::Indeterminate(handle) = &*disposition {
            match (natural, access.reconcile(handle)?) {
                (Natural::Absent, ReconciliationResolution::ExactOld) => {
                    *disposition = Disposition::NotCommitted
                }
                (Natural::Exact, ReconciliationResolution::ExactNew { .. }) => {
                    *disposition = Disposition::Committed
                }
                _ => *disposition = Disposition::Collision,
            }
        }
        Ok(match (&*disposition, natural) {
            (Disposition::NotCommitted, Natural::Absent) => {
                DiscussionCreationAuditOutcome::NotCommitted
            }
            (Disposition::Committed, Natural::Exact) => {
                DiscussionCreationAuditOutcome::Created(self.identity())
            }
            _ => DiscussionCreationAuditOutcome::Collision,
        })
    }
}

#[derive(Clone, Copy)]
enum Natural {
    Absent,
    Exact,
    Collision,
}

#[derive(Clone, Copy)]
enum Access<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

impl Access<'_> {
    fn home_id(self) -> BerylHomeId {
        match self {
            Self::Ordinary(store) => store.home_id(),
            Self::Candidate(access) => access.home_id(),
        }
    }

    fn natural(
        self,
        audit: &DiscussionCreationAudit,
        syndic: &SyndicStorage,
        state: &BerylState,
    ) -> Result<Natural, DiscussionCreationError> {
        let (syndic_status, catalog_status) = match self {
            Self::Ordinary(store) => {
                let before = store.home_revision()?;
                let syndic_status = syndic.discussion_creation_status(store, &audit.0.syndic)?;
                let catalog_status = state
                    .catalog()
                    .initial_publication_status(store, &audit.0.catalog)?;
                if store.home_revision()? != before {
                    return Err(DiscussionCreationError::ConcurrentChange);
                }
                (syndic_status, catalog_status)
            }
            Self::Candidate(access) => {
                let before = access.home_revision()?;
                let syndic_status =
                    syndic.discussion_creation_status_candidate(access, &audit.0.syndic)?;
                let catalog_status = state
                    .catalog()
                    .initial_publication_status_candidate(access, &audit.0.catalog)?;
                if access.home_revision()? != before {
                    return Err(DiscussionCreationError::ConcurrentChange);
                }
                (syndic_status, catalog_status)
            }
        };
        Ok(match (syndic_status, catalog_status) {
            (ThreadCreationStatus::Absent, CatalogInitialStatus::Absent) => Natural::Absent,
            (ThreadCreationStatus::Exact, CatalogInitialStatus::Exact) => Natural::Exact,
            _ => Natural::Collision,
        })
    }

    fn reconcile(
        self,
        handle: &ReconciliationHandle,
    ) -> Result<ReconciliationResolution, DiscussionCreationError> {
        Ok(match self {
            Self::Ordinary(store) => store.retry_reconciliation(handle)?,
            Self::Candidate(access) => access.retry_reconciliation(handle)?,
        })
    }
}
